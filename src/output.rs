use crate::config::{CommandNode, Origin};
use std::io::{IsTerminal, Write};
use std::path::Path;
use std::sync::OnceLock;
use time::{OffsetDateTime, UtcOffset};
use unicode_width::UnicodeWidthStr;

const PRIMARY: &str = "\x1b[38;2;209;224;222m";
const MUTED: &str = "\x1b[38;2;174;178;176m";
const WARNING: &str = "\x1b[38;2;255;213;0m";
const RESET: &str = "\x1b[0m";

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
    let body = format!("{} → {cmd}", paint_prefix(ts));
    if color {
        format!("{MUTED}{body}{RESET}")
    } else {
        body
    }
}

fn fmt_completed(ts: &str, ms: u128, color: bool) -> String {
    let body = format!("{} completed in {ms}ms", paint_prefix(ts));
    if color {
        format!("{MUTED}{body}{RESET}")
    } else {
        body
    }
}

fn fmt_failed(ts: &str, step_idx: usize, exit: i32, color: bool) -> String {
    let body = format!("[jk][{ts}] failed at step {} (exit {})", step_idx + 1, exit);
    if color {
        format!("{WARNING}{body}{RESET}")
    } else {
        body
    }
}

fn fmt_error(msg: &str, color: bool) -> String {
    let body = format!("[jk] error: {msg}");
    if color {
        format!("{WARNING}{body}{RESET}")
    } else {
        body
    }
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
    /// The `[jk] configs:` header is printed only for root listings (`path` is empty)
    /// and suppressed under `JK_QUIET=1`. The command list itself is always printed
    /// (it is data, not decoration), so scripts and CI can rely on bare output.
    pub fn print_listing(
        &self,
        path: &[String],
        children: &std::collections::BTreeMap<String, CommandNode>,
        header_global: Option<&Path>,
        header_local: Option<&Path>,
    ) {
        let mut s = std::io::stdout().lock();

        if path.is_empty() && !self.quiet {
            let bracket = if self.stdout_color {
                format!("{PRIMARY}[jk]{RESET}")
            } else {
                "[jk]".to_string()
            };
            let _ = writeln!(s, "{} configs:", bracket);
            let _ = writeln!(s, "  global: {}", display_path_or_none(header_global));
            let _ = writeln!(s, "  local:  {}", display_path_or_none(header_local));
            let _ = writeln!(s);
        }

        let prefix = if path.is_empty() {
            "jk".to_string()
        } else {
            format!("jk {}", path.join(" "))
        };
        let _ = writeln!(s, "{} commands:", prefix);

        let needs_marker_col = children.values().any(|n| match n {
            CommandNode::Leaf(l) => l.origin != Origin::LocalOnly,
            CommandNode::Namespace(_) => false,
        });

        struct Entry {
            display: String,
            origin: Option<Origin>,
            desc: String,
        }
        let entries: Vec<Entry> = children
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
                Entry {
                    display,
                    origin,
                    desc,
                }
            })
            .collect();

        // Use display width (not byte length) for correct alignment with CJK names.
        let max_w = entries
            .iter()
            .map(|e| UnicodeWidthStr::width(e.display.as_str()))
            .max()
            .unwrap_or(0);

        for e in &entries {
            let pad = max_w.saturating_sub(UnicodeWidthStr::width(e.display.as_str()));

            let marker = if needs_marker_col {
                match e.origin {
                    Some(Origin::GlobalOnly) => "(g) ",
                    Some(Origin::Override) => "(o) ",
                    Some(Origin::LocalOnly) | None => "    ",
                }
            } else {
                ""
            };

            let (open, close) = if self.stdout_color {
                match e.origin {
                    Some(Origin::LocalOnly) => ("\x1b[38;5;117m", "\x1b[0m"),
                    Some(Origin::Override) => ("\x1b[38;5;215m", "\x1b[0m"),
                    Some(Origin::GlobalOnly) | None => ("", ""),
                }
            } else {
                ("", "")
            };

            let _ = writeln!(
                s,
                "  {open}{marker}{display}{close}{pad_spaces}   {desc}",
                open = open,
                marker = marker,
                display = e.display,
                close = close,
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
    fn fmt_step_color_is_single_muted_line() {
        let s = fmt_step("16:45:02.103", "cd src && cargo build --release", true);
        assert_eq!(
            s,
            "\x1b[38;2;174;178;176m[jk][16:45:02.103] → cd src && cargo build --release\x1b[0m"
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
            "\x1b[38;2;174;178;176m[jk][16:45:02.421] completed in 318ms\x1b[0m"
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
