use crate::config::{CommandNode, Origin};
use std::collections::BTreeMap;
use std::fmt::Display;
use std::io::{IsTerminal, Write};
use std::path::Path;
use std::sync::OnceLock;
use time::{OffsetDateTime, UtcOffset};
use unicode_width::UnicodeWidthStr;

const ANSI_PRIMARY: &str = "\x1b[38;2;135;215;255m";
const ANSI_MUTED: &str = "\x1b[38;2;209;224;222m";
const ANSI_WARNING: &str = "\x1b[38;2;255;213;0m";
const ANSI_LOCAL: &str = "\x1b[38;2;163;231;245m";
const ANSI_GLOBAL: &str = "\x1b[38;2;255;175;95m";
const ANSI_UNDERLINE: &str = "\x1b[4m";
const ANSI_RESET: &str = "\x1b[0m";

#[derive(Clone, Copy)]
enum Style {
    Primary,
    Muted,
    Warning,
    LocalOrigin,
    GlobalOrigin,
    Namespace,
}

impl Style {
    fn open(self) -> &'static str {
        match self {
            Style::Primary => ANSI_PRIMARY,
            Style::Muted => ANSI_MUTED,
            Style::Warning => ANSI_WARNING,
            Style::LocalOrigin => ANSI_LOCAL,
            Style::GlobalOrigin => ANSI_GLOBAL,
            Style::Namespace => ANSI_UNDERLINE,
        }
    }
}

#[derive(Clone, Copy)]
struct Painter {
    enabled: bool,
}

impl Painter {
    fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    fn paint(self, style: Style, text: impl Display) -> String {
        if self.enabled {
            format!("{}{}{}", style.open(), text, ANSI_RESET)
        } else {
            text.to_string()
        }
    }
}

// Process-level cache for the local UTC offset. Written once by `init_local_offset()`
// before any threads are spawned; `current_local()` reads it from then on.
//
// The `time` crate's `UtcOffset::current_local_offset()` can return `IndeterminateOffset`
// in a multi-threaded process on Linux because `getenv` is not thread-safe. Capturing
// the offset at startup avoids this. Falls back to UTC if the offset cannot be obtained.
static LOCAL_OFFSET: OnceLock<UtcOffset> = OnceLock::new();

pub fn init_local_offset() {
    let off = UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC);
    let _ = LOCAL_OFFSET.set(off);
}

fn current_local() -> OffsetDateTime {
    let off = LOCAL_OFFSET.get().copied().unwrap_or(UtcOffset::UTC);
    OffsetDateTime::now_utc().to_offset(off)
}

fn format_timestamp(t: OffsetDateTime) -> String {
    let ms = t.nanosecond() / 1_000_000;
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        t.hour(),
        t.minute(),
        t.second(),
        ms
    )
}

fn paint_prefix(ts: &str) -> String {
    format!("[jk][{ts}]")
}

fn fmt_step(ts: &str, cmd: &str, color: bool) -> String {
    let p = Painter::new(color);
    format!(
        "{}{}",
        p.paint(Style::Muted, format!("{} → ", paint_prefix(ts))),
        p.paint(Style::Primary, cmd),
    )
}

fn fmt_completed(ts: &str, ms: u128, color: bool) -> String {
    let p = Painter::new(color);
    let body = format!("{} completed in {ms}ms", paint_prefix(ts));
    p.paint(Style::Muted, body)
}

fn fmt_failed(ts: &str, step_idx: usize, exit: i32, color: bool) -> String {
    let p = Painter::new(color);
    let body = format!("[jk][{ts}] failed at step {} (exit {})", step_idx + 1, exit);
    p.paint(Style::Warning, body)
}

fn fmt_error(msg: &str, color: bool) -> String {
    let p = Painter::new(color);
    let body = format!("[jk] error: {msg}");
    p.paint(Style::Warning, body)
}

struct ListingEntry {
    display: String,
    origin: Option<Origin>,
    desc: String,
}

impl ListingEntry {
    fn marker(&self, needs_marker_col: bool) -> &'static str {
        if !needs_marker_col {
            return "";
        }
        match self.origin {
            Some(Origin::GlobalOnly) => "(g) ",
            Some(Origin::Override) => "(o) ",
            Some(Origin::LocalOnly) | None => "    ",
        }
    }

    fn styled_label(&self, needs_marker_col: bool, painter: Painter) -> String {
        let marker = self.marker(needs_marker_col);
        match self.origin {
            Some(Origin::LocalOnly) => {
                painter.paint(Style::LocalOrigin, format!("{marker}{}", self.display))
            }
            Some(Origin::Override) => {
                painter.paint(Style::Warning, format!("{marker}{}", self.display))
            }
            Some(Origin::GlobalOnly) => {
                painter.paint(Style::GlobalOrigin, format!("{marker}{}", self.display))
            }
            None => format!("{marker}{}", painter.paint(Style::Namespace, &self.display)),
        }
    }

    fn width(&self) -> usize {
        UnicodeWidthStr::width(self.display.as_str())
    }
}

fn listing_entries(children: &BTreeMap<String, CommandNode>) -> Vec<ListingEntry> {
    children
        .iter()
        .map(|(k, v)| {
            let (display, origin, desc) = match v {
                CommandNode::Namespace(_) => (format!("{}/", k), None, String::new()),
                CommandNode::Leaf(l) => (
                    k.clone(),
                    Some(l.origin),
                    l.desc.clone().unwrap_or_default(),
                ),
            };
            ListingEntry {
                display,
                origin,
                desc,
            }
        })
        .collect()
}

fn command_header(path: &[String]) -> String {
    let prefix = if path.is_empty() {
        "jk".to_string()
    } else {
        format!("jk {}", path.join(" "))
    };
    format!("{prefix} commands:")
}

pub struct Out {
    quiet: bool,
    color: bool,
    stdout_color: bool,
}

impl Out {
    /// `JK_QUIET` and `JK_NO_COLOR` activate only when the value is exactly `"1"`.
    /// Any other value (including empty string, `"0"`, `"true"`, `"yes"`) is ignored.
    pub fn from_env() -> Self {
        let quiet_var = std::env::var("JK_QUIET").ok();
        let color_var = std::env::var("JK_NO_COLOR").ok();
        let stderr_tty = std::io::stderr().is_terminal();
        let stdout_tty = std::io::stdout().is_terminal();
        Self::from_env_parts(
            quiet_var.as_deref(),
            color_var.as_deref(),
            stderr_tty,
            stdout_tty,
        )
    }

    pub fn from_env_parts(
        quiet_var: Option<&str>,
        color_var: Option<&str>,
        stderr_tty: bool,
        stdout_tty: bool,
    ) -> Self {
        let quiet = quiet_var == Some("1");
        let force_no_color = color_var == Some("1");
        Self {
            quiet,
            color: !force_no_color && stderr_tty,
            stdout_color: !force_no_color && stdout_tty,
        }
    }

    pub fn step_header(&self, cmd: &str) {
        if self.quiet {
            return;
        }
        let ts = format_timestamp(current_local());
        let _ = writeln!(
            std::io::stderr().lock(),
            "{}",
            fmt_step(&ts, cmd, self.color)
        );
    }

    pub fn completed(&self, ms: u128) {
        if self.quiet {
            return;
        }
        let ts = format_timestamp(current_local());
        let _ = writeln!(
            std::io::stderr().lock(),
            "{}",
            fmt_completed(&ts, ms, self.color)
        );
    }

    pub fn failed(&self, step_idx: usize, exit: i32) {
        if self.quiet {
            return;
        }
        let ts = format_timestamp(current_local());
        let _ = writeln!(
            std::io::stderr().lock(),
            "{}",
            fmt_failed(&ts, step_idx, exit, self.color)
        );
    }

    pub fn user_error(&self, msg: &str) {
        let _ = writeln!(std::io::stderr().lock(), "{}", fmt_error(msg, self.color));
    }

    #[cfg(test)]
    pub fn quiet(&self) -> bool {
        self.quiet
    }

    #[cfg(test)]
    pub fn color(&self) -> bool {
        self.color
    }

    #[cfg(test)]
    pub fn stdout_color(&self) -> bool {
        self.stdout_color
    }

    /// Print a command listing to stdout.
    ///
    /// The `jk configs:` header is printed only for root listings (`path` is empty)
    /// and suppressed under `JK_QUIET=1`. The command list itself is always printed
    /// (it is data, not decoration), so scripts and CI can rely on bare output.
    pub fn print_listing(
        &self,
        path: &[String],
        children: &BTreeMap<String, CommandNode>,
        header_global: Option<&Path>,
        header_local: Option<&Path>,
    ) {
        let mut s = std::io::stdout().lock();
        let painter = Painter::new(self.stdout_color);

        if path.is_empty() && !self.quiet {
            let _ = writeln!(s, "{}", painter.paint(Style::Primary, "jk configs:"));
            let _ = writeln!(s, "  global: {}", display_path_or_none(header_global));
            let _ = writeln!(s, "  local:  {}", display_path_or_none(header_local));
            let _ = writeln!(s);
        }

        let commands_header = command_header(path);
        let commands_header = if path.is_empty() {
            painter.paint(Style::Primary, commands_header)
        } else {
            commands_header
        };
        let _ = writeln!(s, "{commands_header}");

        let entries = listing_entries(children);
        let needs_marker_col = entries
            .iter()
            .any(|e| matches!(e.origin, Some(Origin::GlobalOnly | Origin::Override)));

        let max_w = entries.iter().map(ListingEntry::width).max().unwrap_or(0);

        for e in &entries {
            let label = e.styled_label(needs_marker_col, painter);
            let pad = max_w.saturating_sub(e.width());

            let _ = writeln!(
                s,
                "  {label}{pad_spaces}   {desc}",
                label = label,
                pad_spaces = " ".repeat(pad),
                desc = e.desc,
            );
        }
    }
}

fn display_path_or_none(p: Option<&Path>) -> String {
    match p {
        Some(path) => path.display().to_string(),
        None => "(none)".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jk_quiet_only_activates_on_exact_one() {
        assert!(!Out::from_env_parts(None, None, true, true).quiet());
        assert!(!Out::from_env_parts(Some(""), None, true, true).quiet());
        assert!(!Out::from_env_parts(Some("0"), None, true, true).quiet());
        assert!(!Out::from_env_parts(Some("true"), None, true, true).quiet());
        assert!(!Out::from_env_parts(Some("yes"), None, true, true).quiet());
        assert!(!Out::from_env_parts(Some("on"), None, true, true).quiet());
        assert!(!Out::from_env_parts(Some("1 "), None, true, true).quiet());
        assert!(Out::from_env_parts(Some("1"), None, true, true).quiet());
    }

    #[test]
    fn jk_no_color_only_activates_on_exact_one() {
        assert!(Out::from_env_parts(None, None, true, true).color());
        assert!(Out::from_env_parts(None, Some(""), true, true).color());
        assert!(Out::from_env_parts(None, Some("0"), true, true).color());
        assert!(Out::from_env_parts(None, Some("true"), true, true).color());
        assert!(Out::from_env_parts(None, Some("yes"), true, true).color());
        assert!(!Out::from_env_parts(None, Some("1"), true, true).color());
        assert!(!Out::from_env_parts(None, Some("1"), true, true).stdout_color());
    }

    #[test]
    fn color_off_when_stderr_not_tty() {
        assert!(!Out::from_env_parts(None, None, false, true).color());
        assert!(!Out::from_env_parts(None, Some("0"), false, true).color());
        assert!(!Out::from_env_parts(None, Some("1"), false, true).color());
    }

    #[test]
    fn stdout_color_independent_from_stderr() {
        let out = Out::from_env_parts(None, None, true, false);
        assert!(out.color(), "stderr_tty=true → stderr color on");
        assert!(!out.stdout_color(), "stdout_tty=false → stdout color off");

        let out = Out::from_env_parts(None, None, false, true);
        assert!(!out.color(), "stderr_tty=false → stderr color off");
        assert!(out.stdout_color(), "stdout_tty=true → stdout color on");
    }

    #[test]
    fn fmt_step_no_color() {
        assert_eq!(
            fmt_step("16:45:02.103", "cd src && cargo build --release", false),
            "[jk][16:45:02.103] → cd src && cargo build --release"
        );
    }

    #[test]
    fn fmt_step_color_splits_prefix_and_command() {
        let s = fmt_step("16:45:02.103", "cd src && cargo build --release", true);
        assert_eq!(
            s,
            "\x1b[38;2;209;224;222m[jk][16:45:02.103] → \x1b[0m\x1b[38;2;135;215;255mcd src && cargo build --release\x1b[0m"
        );
    }

    #[test]
    fn fmt_completed_no_color() {
        assert_eq!(
            fmt_completed("16:45:02.421", 318, false),
            "[jk][16:45:02.421] completed in 318ms"
        );
    }

    #[test]
    fn fmt_completed_color_is_single_muted_line() {
        let s = fmt_completed("16:45:02.421", 318, true);
        assert_eq!(
            s,
            "\x1b[38;2;209;224;222m[jk][16:45:02.421] completed in 318ms\x1b[0m"
        );
    }

    #[test]
    fn fmt_failed_no_color() {
        assert_eq!(
            fmt_failed("16:45:02.421", 0, 2, false),
            "[jk][16:45:02.421] failed at step 1 (exit 2)"
        );
    }

    #[test]
    fn fmt_failed_color_whole_line_warning() {
        let s = fmt_failed("16:45:02.421", 0, 2, true);
        assert!(
            s.starts_with("\x1b[38;2;255;213;0m"),
            "failed line should start with WARNING open; got: {s}"
        );
        assert!(
            s.ends_with("\x1b[0m"),
            "failed line should end with reset; got: {s}"
        );
        let inner = &s["\x1b[38;2;255;213;0m".len()..s.len() - "\x1b[0m".len()];
        assert!(
            !inner.contains("\x1b["),
            "failed line should be single color; got inner: {inner}"
        );
        assert!(
            inner.contains("[jk][16:45:02.421] failed at step 1 (exit 2)"),
            "got inner: {inner}"
        );
    }

    #[test]
    fn fmt_error_no_color_no_timestamp() {
        assert_eq!(
            fmt_error("config path invalid: /tmp/nope", false),
            "[jk] error: config path invalid: /tmp/nope"
        );
    }

    #[test]
    fn fmt_error_color_whole_line_warning() {
        let s = fmt_error("config path invalid: /tmp/nope", true);
        assert!(s.starts_with("\x1b[38;2;255;213;0m"), "got: {s}");
        assert!(s.ends_with("\x1b[0m"), "got: {s}");
        let inner = &s["\x1b[38;2;255;213;0m".len()..s.len() - "\x1b[0m".len()];
        assert!(
            !inner.contains("\x1b["),
            "error line should be single color; got inner: {inner}"
        );
        assert!(
            inner.contains("[jk] error: config path invalid: /tmp/nope"),
            "got inner: {inner}"
        );
    }

    #[test]
    fn listing_label_applies_semantic_styles() {
        let p = Painter::new(true);

        let local = ListingEntry {
            display: "local".into(),
            origin: Some(Origin::LocalOnly),
            desc: String::new(),
        };
        assert_eq!(
            local.styled_label(true, p),
            format!("{ANSI_LOCAL}    local{ANSI_RESET}")
        );

        let global = ListingEntry {
            display: "global".into(),
            origin: Some(Origin::GlobalOnly),
            desc: String::new(),
        };
        assert_eq!(
            global.styled_label(true, p),
            format!("{ANSI_GLOBAL}(g) global{ANSI_RESET}")
        );

        let namespace = ListingEntry {
            display: "tools/".into(),
            origin: None,
            desc: String::new(),
        };
        assert_eq!(
            namespace.styled_label(true, p),
            format!("    {ANSI_UNDERLINE}tools/{ANSI_RESET}")
        );
    }

    #[test]
    fn fmt_timestamp_pads_and_truncates_to_millis() {
        use time::macros::datetime;
        let t = datetime!(2026-05-06 16:45:01.385_500 UTC);
        assert_eq!(format_timestamp(t), "16:45:01.385");
        let t = datetime!(2026-05-06 00:00:00.000 UTC);
        assert_eq!(format_timestamp(t), "00:00:00.000");
        let t = datetime!(2026-05-06 09:08:07.006 UTC);
        assert_eq!(format_timestamp(t), "09:08:07.006");
    }
}
