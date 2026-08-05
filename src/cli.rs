use crate::error::{JkError, JkResult};

#[derive(Debug, Default, PartialEq)]
pub struct ParsedCli {
    pub path: Vec<String>,
    pub dry_run: bool,
    pub quiet: bool,
    pub no_color: bool,
    pub init: bool,
    pub version: bool,
    pub config_path: Option<String>,
    pub home_path: Option<String>,
}

pub fn parse_argv(argv: Vec<String>) -> JkResult<ParsedCli> {
    let mut out = ParsedCli::default();
    let mut positional: Vec<String> = Vec::new();
    let mut after_sep = false;

    for tok in argv {
        if after_sep {
            positional.push(tok);
            continue;
        }
        if tok == "--" {
            after_sep = true;
            continue;
        }
        let Some(rest) = tok.strip_prefix("++") else {
            positional.push(tok);
            continue;
        };
        match rest.split_once('=') {
            Some((name, value)) => {
                let full = format!("++{}", name);
                match full.as_str() {
                    "++config" | "++home" => {
                        if value.is_empty() {
                            return Err(JkError::MalformedFlag {
                                name: full,
                                reason: "value cannot be empty".into(),
                            });
                        }
                        let slot = if full == "++config" {
                            &mut out.config_path
                        } else {
                            &mut out.home_path
                        };
                        if slot.is_some() {
                            return Err(JkError::MalformedFlag {
                                name: full,
                                reason: "may only be specified once".into(),
                            });
                        }
                        *slot = Some(value.to_string());
                    }
                    "++dry-run" | "++quiet" | "++no-color" | "++init" | "++version" => {
                        return Err(JkError::MalformedFlag {
                            name: full,
                            reason: "boolean flag does not take a value".into(),
                        });
                    }
                    _ => return Err(JkError::UnknownFlag(full)),
                }
            }
            None => {
                if rest.is_empty() {
                    // `++` alone has no flag name — malformed, not unknown.
                    return Err(JkError::MalformedFlag {
                        name: "++".into(),
                        reason: "missing flag name after '++'".into(),
                    });
                }
                let full = format!("++{}", rest);
                match full.as_str() {
                    "++dry-run" => out.dry_run = true,
                    "++quiet" => out.quiet = true,
                    "++no-color" => out.no_color = true,
                    "++init" => out.init = true,
                    "++version" => out.version = true,
                    "++config" | "++home" => {
                        return Err(JkError::MalformedFlag {
                            name: full,
                            reason: "expected '=<value>'".into(),
                        });
                    }
                    _ => return Err(JkError::UnknownFlag(full)),
                }
            }
        }
    }

    out.path = positional;
    if out.init {
        if !out.path.is_empty() {
            return Err(JkError::MalformedFlag {
                name: "++init".into(),
                reason: "does not take command arguments".into(),
            });
        }
        if out.dry_run
            || out.quiet
            || out.no_color
            || out.version
            || out.config_path.is_some()
            || out.home_path.is_some()
        {
            return Err(JkError::MalformedFlag {
                name: "++init".into(),
                reason: "cannot be combined with other jk flags".into(),
            });
        }
    }
    Ok(out)
}
