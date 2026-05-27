use crate::error::{JkError, JkResult};
use crate::shell::Shell;
use std::collections::BTreeSet;

/// `max_n` is 0 when no positional placeholders are used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaceholderShape {
    pub max_n: usize,
    pub has_at: bool,
}

struct ParsedTemplate {
    indices: BTreeSet<usize>,
    has_at: bool,
}

fn parse_template(template: &str) -> JkResult<ParsedTemplate> {
    let mut indices: BTreeSet<usize> = BTreeSet::new();
    let mut has_at = false;

    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        // Only the two-character sequence `#{` triggers placeholder scanning.
        // A lone `#` or `{` is literal, so `${VAR}`, `{{X}}`, awk blocks, etc.
        // are never interpreted as placeholders.
        if c != '#' || chars.peek() != Some(&'{') {
            continue;
        }
        chars.next(); // consume '{'

        // Scan inner until '}'
        let mut inner = String::new();
        let mut closed = false;
        while let Some(&nc) = chars.peek() {
            chars.next();
            if nc == '}' {
                closed = true;
                break;
            }
            inner.push(nc);
        }
        if !closed {
            return Err(JkError::ConfigSchema(format!(
                "unclosed placeholder: '#{{{}'",
                inner
            )));
        }

        let (kind_str, _raw) = if let Some(stripped) = inner.strip_suffix('!') {
            (stripped, true)
        } else {
            (inner.as_str(), false)
        };

        if kind_str == "@" {
            has_at = true;
        } else if let Ok(n) = kind_str.parse::<usize>() {
            // Strict numeric: parsed value must round-trip ("01" / "+1" rejected).
            if n == 0 || kind_str != n.to_string() {
                return Err(JkError::ConfigSchema(format!(
                    "invalid placeholder '#{{{}}}' (indices are 1-based positive integers)",
                    inner
                )));
            }
            indices.insert(n);
        } else {
            return Err(JkError::ConfigSchema(format!(
                "invalid placeholder '#{{{}}}'",
                inner
            )));
        }
    }

    Ok(ParsedTemplate { indices, has_at })
}

/// Index contiguity is not checked here — that is enforced by `aggregate_shape`.
pub fn analyze(template: &str) -> JkResult<PlaceholderShape> {
    let parsed = parse_template(template)?;
    let max_n = parsed.indices.iter().max().copied().unwrap_or(0);
    Ok(PlaceholderShape { max_n, has_at: parsed.has_at })
}

/// Enforce leaf-level index contiguity across all items in a sequence.
/// `["echo #{1}", "echo #{2}"]` is valid even though each item looks incomplete.
pub fn aggregate_shape(templates: &[&str]) -> JkResult<PlaceholderShape> {
    let mut all_indices: BTreeSet<usize> = BTreeSet::new();
    let mut has_at = false;
    for t in templates {
        let parsed = parse_template(t)?;
        all_indices.extend(parsed.indices);
        if parsed.has_at {
            has_at = true;
        }
    }
    let max_n = all_indices.iter().max().copied().unwrap_or(0);
    for i in 1..=max_n {
        if !all_indices.contains(&i) {
            return Err(JkError::ConfigSchema(format!(
                "non-contiguous placeholder indices: missing #{{{}}} (max index used is #{{{}}})",
                i, max_n
            )));
        }
    }
    Ok(PlaceholderShape { max_n, has_at })
}

pub fn validate_args(args: &[String], shape: PlaceholderShape) -> JkResult<()> {
    if args.len() < shape.max_n {
        return Err(JkError::MissingArg(format!("#{{{}}}", args.len() + 1)));
    }
    if args.len() > shape.max_n && !shape.has_at {
        return Err(JkError::ExtraArgs(args.len() - shape.max_n));
    }
    Ok(())
}

/// `#{@}` expands to `args[this_item_max_n..]` — remaining args after the highest
/// positional index used in *this item*, not the leaf aggregate.
pub fn substitute(template: &str, args: &[String], shell: Shell) -> JkResult<String> {
    let shape = analyze(template)?;

    let mut out = String::new();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '#' {
            out.push(c);
            continue;
        }
        if chars.peek() != Some(&'{') {
            out.push('#');
            continue;
        }
        chars.next(); // consume '{'

        let mut inner = String::new();
        while let Some(&nc) = chars.peek() {
            chars.next();
            if nc == '}' {
                break;
            }
            inner.push(nc);
        }

        let (kind_str, raw) = if let Some(stripped) = inner.strip_suffix('!') {
            (stripped, true)
        } else {
            (inner.as_str(), false)
        };

        if kind_str == "@" {
            let slice = &args[shape.max_n..];
            if raw {
                out.push_str(&slice.join(" "));
            } else {
                let parts: Vec<String> = slice.iter().map(|a| shell.quote(a)).collect();
                out.push_str(&parts.join(" "));
            }
        } else {
            let n: usize = kind_str.parse().expect("analyze validated this is a positive integer");
            let v = &args[n - 1];
            if raw {
                out.push_str(v);
            } else {
                out.push_str(&shell.quote(v));
            }
        }
    }

    Ok(out)
}

pub fn fold(s: &str) -> String {
    let parts: Vec<String> = s
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    parts.join(" ")
}
