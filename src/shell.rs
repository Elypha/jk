use crate::error::{JkError, JkResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    Bash,
    Sh,
    Zsh,
    Pwsh,
    Fish,
}

/// Everything needed to spawn a shell: program name, argv flags, and env vars to unset.
///
/// `env_remove` closes profile-isolation gaps: bash reads `$BASH_ENV` even with
/// `--noprofile --norc`, and `sh` reads `$ENV`, so both must be explicitly unset.
pub struct ShellInvocation {
    pub program: &'static str,
    pub args: &'static [&'static str],
    pub env_remove: &'static [&'static str],
}

impl Shell {
    pub fn parse(name: &str) -> JkResult<Shell> {
        match name {
            "bash" => Ok(Shell::Bash),
            "sh" => Ok(Shell::Sh),
            "zsh" => Ok(Shell::Zsh),
            "pwsh" => Ok(Shell::Pwsh),
            "fish" => Ok(Shell::Fish),
            other => Err(JkError::ConfigSchema(format!("unsupported shell: {}", other))),
        }
    }

    pub fn invocation(&self) -> ShellInvocation {
        match self {
            Shell::Bash => ShellInvocation {
                program: "bash",
                args: &["--noprofile", "--norc", "-c"],
                env_remove: &["BASH_ENV"],
            },
            Shell::Sh => ShellInvocation {
                program: "sh",
                args: &["-c"],
                env_remove: &["ENV"],
            },
            Shell::Zsh => ShellInvocation {
                program: "zsh",
                args: &["--no-rcs", "--no-globalrcs", "-c"],
                env_remove: &[],
            },
            Shell::Pwsh => ShellInvocation {
                program: "pwsh",
                args: &["-NoLogo", "-NoProfile", "-Command"],
                env_remove: &[],
            },
            Shell::Fish => ShellInvocation {
                program: "fish",
                args: &["--no-config", "-c"],
                env_remove: &[],
            },
        }
    }

    pub fn quote(&self, raw: &str) -> String {
        match self {
            Shell::Bash | Shell::Sh | Shell::Zsh | Shell::Fish => quote_posix(raw),
            Shell::Pwsh => quote_pwsh(raw),
        }
    }
}

fn quote_posix(s: &str) -> String {
    if s.is_empty() {
        return "''".into();
    }
    if s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '/' | '.' | ':' | '=' | ',')) {
        return s.into();
    }
    // Single-quote wrap; interior `'` becomes `'\''`.
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if c == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

fn quote_pwsh(s: &str) -> String {
    if !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '/' | '.' | ':' | '=' | ','))
    {
        return s.into();
    }
    if s.is_empty() {
        return "''".into();
    }
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if c == '\'' {
            out.push_str("''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn posix_quote_simple() {
        assert_eq!(quote_posix("hello"), "hello");
        assert_eq!(quote_posix("hello world"), "'hello world'");
        assert_eq!(quote_posix("it's"), "'it'\\''s'");
    }

    #[test]
    fn pwsh_quote() {
        assert_eq!(quote_pwsh("hello"), "hello");
        assert_eq!(quote_pwsh("hello world"), "'hello world'");
        assert_eq!(quote_pwsh("it's"), "'it''s'");
    }

    #[test]
    fn shell_parse() {
        assert!(matches!(Shell::parse("bash"), Ok(Shell::Bash)));
        assert!(Shell::parse("zsh4").is_err());
        assert!(Shell::parse("cmd").is_err());
    }

    #[test]
    fn invocation_has_env_remove_for_bash_and_sh() {
        let bash = Shell::Bash.invocation();
        assert!(bash.env_remove.contains(&"BASH_ENV"));
        let sh = Shell::Sh.invocation();
        assert!(sh.env_remove.contains(&"ENV"));
    }
}
