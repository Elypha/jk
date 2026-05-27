use crate::error::{JkError, JkResult};

#[derive(Debug, Default, PartialEq)]
pub struct ParsedCli {
    pub path: Vec<String>,
    pub dry_run: bool,
    pub version: bool,
    pub config_path: Option<String>,
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
                    "++config" => {
                        if value.is_empty() {
                            return Err(JkError::MalformedFlag {
                                name: full,
                                reason: "value cannot be empty".into(),
                            });
                        }
                        if out.config_path.is_some() {
                            return Err(JkError::MalformedFlag {
                                name: full,
                                reason: "may only be specified once".into(),
                            });
                        }
                        out.config_path = Some(value.to_string());
                    }
                    "++dry-run" | "++version" => {
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
                    "++version" => out.version = true,
                    "++config" => {
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
    Ok(out)
}
