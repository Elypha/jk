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
const ANSI_GLOBAL: &str = "\x1b[38;2;240;190;138m";
const ANSI_LOCAL_NAMESPACE: &str = "\x1b[38;2;163;231;245m\x1b[4m";
const ANSI_GLOBAL_NAMESPACE: &str = "\x1b[38;2;240;190;138m\x1b[4m";
const ANSI_DIM: &str = "\x1b[2m";
const ANSI_DIM_STRIKETHROUGH: &str = "\x1b[2;9m";
const ANSI_RESET: &str = "\x1b[0m";

#[derive(Clone, Copy)]
enum Style {
    Primary,
    Muted,
    Warning,
    LocalOrigin,
    GlobalOrigin,
    LocalNamespace,
    GlobalNamespace,
    Dim,
    DimStrikethrough,
}

impl Style {
    fn open(self) -> &'static str {
        match self {
            Style::Primary => ANSI_PRIMARY,
            Style::Muted => ANSI_MUTED,
            Style::Warning => ANSI_WARNING,
            Style::LocalOrigin => ANSI_LOCAL,
            Style::GlobalOrigin => ANSI_GLOBAL,
            Style::LocalNamespace => ANSI_LOCAL_NAMESPACE,
            Style::GlobalNamespace => ANSI_GLOBAL_NAMESPACE,
            Style::Dim => ANSI_DIM,
            Style::DimStrikethrough => ANSI_DIM_STRIKETHROUGH,
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

#[derive(Clone, Copy)]
enum ListingSection {
    Global,
    Local,
}

impl ListingSection {
    fn includes(self, origin: Origin) -> bool {
        match self {
            Self::Global => matches!(origin, Origin::GlobalOnly | Origin::Override),
            Self::Local => matches!(origin, Origin::LocalOnly | Origin::Override),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Local => "local",
        }
    }
}

struct ListingEntry {
    depth: usize,
    display: String,
    origin: Option<Origin>,
    desc: Vec<String>,
}

impl ListingEntry {
    fn is_override_placeholder(&self, section: ListingSection) -> bool {
        matches!(section, ListingSection::Global) && self.origin == Some(Origin::Override)
    }

    fn styled_label(&self, section: ListingSection, painter: Painter) -> String {
        let indent = "    ".repeat(self.depth);
        match self.origin {
            Some(Origin::LocalOnly) => {
                format!(
                    "{indent}{}",
                    painter.paint(Style::LocalOrigin, &self.display)
                )
            }
            Some(Origin::Override) => {
                let style = if self.is_override_placeholder(section) {
                    Style::DimStrikethrough
                } else {
                    Style::Warning
                };
                format!("{indent}{}", painter.paint(style, &self.display))
            }
            Some(Origin::GlobalOnly) => {
                format!(
                    "{indent}{}",
                    painter.paint(Style::GlobalOrigin, &self.display)
                )
            }
            None => {
                let style = match section {
                    ListingSection::Global => Style::GlobalNamespace,
                    ListingSection::Local => Style::LocalNamespace,
                };
                format!("{indent}{}", painter.paint(style, &self.display))
            }
        }
    }

    fn styled_desc(
        &self,
        line: &str,
        section: ListingSection,
        painter: Painter,
    ) -> String {
        if self.is_override_placeholder(section) {
            painter.paint(Style::Dim, line)
        } else {
            line.to_string()
        }
    }

    fn width(&self) -> usize {
        self.depth * 4 + UnicodeWidthStr::width(self.display.as_str())
    }
}

fn listing_entries(
    children: &BTreeMap<String, CommandNode>,
    section: ListingSection,
) -> Vec<ListingEntry> {
    fn collect(
        children: &BTreeMap<String, CommandNode>,
        section: ListingSection,
        depth: usize,
        entries: &mut Vec<ListingEntry>,
    ) {
        // Groups come first at every level; BTreeMap keeps each kind alphabetical.
        for (name, node) in children {
            let CommandNode::Namespace(grandchildren) = node else {
                continue;
            };
            let namespace_index = entries.len();
            entries.push(ListingEntry {
                depth,
                display: format!("{name}/"),
                origin: None,
                desc: Vec::new(),
            });
            collect(grandchildren, section, depth + 1, entries);
            if entries.len() == namespace_index + 1 {
                entries.pop();
            }
        }

        for (name, node) in children {
            if let CommandNode::Leaf(leaf) = node {
                if section.includes(leaf.origin) {
                    entries.push(ListingEntry {
                        depth,
                        display: name.clone(),
                        origin: Some(leaf.origin),
                        desc: if matches!(section, ListingSection::Global)
                            && leaf.origin == Origin::Override
                        {
                            vec!["-> local".to_string()]
                        } else {
                            leaf.desc.clone()
                        },
                    });
                }
            }
        }
    }

    let mut entries = Vec::new();
    collect(children, section, 0, &mut entries);
    entries
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
        let quiet = quiet_var.as_deref() == Some("1");
        let force_no_color = color_var.as_deref() == Some("1");
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

    /// Print a command listing to stdout.
    ///
    /// Commands are grouped by their effective global/local origin and namespaces
    /// are expanded recursively. `JK_QUIET=1` keeps the source sections but omits
    /// their config paths.
    pub fn print_listing(
        &self,
        path: &[String],
        children: &BTreeMap<String, CommandNode>,
        header_global: Option<&Path>,
        header_local: Option<&Path>,
    ) {
        let mut s = std::io::stdout().lock();
        let painter = Painter::new(self.stdout_color);

        let sections = [
            (
                ListingSection::Global,
                listing_entries(children, ListingSection::Global),
            ),
            (
                ListingSection::Local,
                listing_entries(children, ListingSection::Local),
            ),
        ];
        let max_w = sections
            .iter()
            .flat_map(|(_, entries)| entries)
            .map(ListingEntry::width)
            .max()
            .unwrap_or(0);

        for (section, entries) in &sections {
            if !path.is_empty() && entries.is_empty() {
                continue;
            }

            let heading = if self.quiet {
                format!("--- {} commands ---", section.name())
            } else {
                let section_path = match section {
                    ListingSection::Global => header_global,
                    ListingSection::Local => header_local,
                };
                format!(
                    "--- {} commands: {} ---",
                    section.name(),
                    display_path_or_none(section_path)
                )
            };
            let _ = writeln!(s, "{}", painter.paint(Style::Primary, heading));

            for entry in entries {
                let label = entry.styled_label(*section, painter);
                if entry.desc.is_empty() {
                    let _ = writeln!(s, "{label}");
                    continue;
                }

                let pad = max_w.saturating_sub(entry.width());
                for (i, line) in entry.desc.iter().enumerate() {
                    let desc = entry.styled_desc(line, *section, painter);
                    if i == 0 {
                        let _ = writeln!(
                            s,
                            "{label}{pad_spaces}   {desc}",
                            pad_spaces = " ".repeat(pad),
                        );
                    } else {
                        let _ = writeln!(s, "{indent}{desc}", indent = " ".repeat(max_w + 3));
                    }
                }
            }
        }
    }
}

fn display_path_or_none(p: Option<&Path>) -> String {
    match p {
        Some(path) => path.display().to_string(),
        None => "(none)".to_string(),
    }
}
