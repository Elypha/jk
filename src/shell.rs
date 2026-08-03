use crate::error::{JkError, JkResult};
#[cfg(windows)]
use std::path::Path;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    Bash,
    Bun,
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
    pub program: PathBuf,
    pub args: &'static [&'static str],
    pub env_remove: &'static [&'static str],
}

impl Shell {
    pub fn parse(name: &str) -> JkResult<Shell> {
        match name {
            "bash" => Ok(Shell::Bash),
            "bun" => Ok(Shell::Bun),
            "sh" => Ok(Shell::Sh),
            "zsh" => Ok(Shell::Zsh),
            "pwsh" => Ok(Shell::Pwsh),
            "fish" => Ok(Shell::Fish),
            other => Err(JkError::ConfigSchema(format!("unsupported shell: {other}"))),
        }
    }

    pub fn invocation(&self) -> JkResult<ShellInvocation> {
        let invocation = match self {
            Shell::Bash => ShellInvocation {
                program: bash_program()?,
                args: &["--noprofile", "--norc", "-c"],
                env_remove: &["BASH_ENV"],
            },
            Shell::Bun => ShellInvocation {
                program: PathBuf::from("bun"),
                args: &["--no-env-file", "exec"],
                env_remove: &["BUN_OPTIONS"],
            },
            Shell::Sh => ShellInvocation {
                program: PathBuf::from("sh"),
                args: &["-c"],
                env_remove: &["ENV"],
            },
            Shell::Zsh => ShellInvocation {
                program: PathBuf::from("zsh"),
                args: &["--no-rcs", "--no-globalrcs", "-c"],
                env_remove: &[],
            },
            Shell::Pwsh => ShellInvocation {
                program: PathBuf::from("pwsh"),
                args: &["-NoLogo", "-NoProfile", "-Command"],
                env_remove: &[],
            },
            Shell::Fish => ShellInvocation {
                program: PathBuf::from("fish"),
                args: &["--no-config", "-c"],
                env_remove: &[],
            },
        };
        Ok(invocation)
    }

    pub fn quote(&self, raw: &str) -> String {
        match self {
            Shell::Bash | Shell::Sh | Shell::Zsh | Shell::Fish => quote_posix(raw),
            Shell::Bun => quote_bun(raw),
            Shell::Pwsh => quote_pwsh(raw),
        }
    }
}

#[cfg(not(windows))]
fn bash_program() -> JkResult<PathBuf> {
    Ok(PathBuf::from("bash"))
}

#[cfg(windows)]
fn bash_program() -> JkResult<PathBuf> {
    resolve_git_for_windows_root()
        .map(|root| root.join("bin").join("bash.exe"))
        .ok_or_else(|| {
            JkError::SpawnFailed(
                "'bash': Git for Windows Bash was not found; install Git for Windows or set GIT_INSTALL_ROOT"
                    .into(),
            )
        })
}

#[cfg(windows)]
fn resolve_git_for_windows_root() -> Option<PathBuf> {
    if let Some(root) = std::env::var_os("GIT_INSTALL_ROOT")
        .map(PathBuf::from)
        .filter(|root| valid_git_for_windows_root(root))
    {
        return Some(root);
    }

    if let Some(root) = git_exec_path().and_then(|path| git_root_from_exec_path(&path)) {
        return Some(root);
    }

    common_git_for_windows_roots()
        .into_iter()
        .find(|root| valid_git_for_windows_root(root))
}

#[cfg(windows)]
fn git_exec_path() -> Option<PathBuf> {
    let output = std::process::Command::new("git")
        .arg("--exec-path")
        .env_remove("GIT_EXEC_PATH")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let path = String::from_utf8(output.stdout).ok()?;
    let path = path.trim();
    (!path.is_empty()).then(|| PathBuf::from(path))
}

#[cfg(windows)]
fn git_root_from_exec_path(exec_path: &Path) -> Option<PathBuf> {
    exec_path
        .ancestors()
        .find(|root| valid_git_for_windows_root(root))
        .map(Path::to_path_buf)
}

#[cfg(windows)]
fn common_git_for_windows_roots() -> Vec<PathBuf> {
    let mut roots = Vec::with_capacity(4);
    if let Some(path) = std::env::var_os("ProgramFiles") {
        roots.push(PathBuf::from(path).join("Git"));
    }
    if let Some(path) = std::env::var_os("LOCALAPPDATA") {
        roots.push(PathBuf::from(path).join("Programs").join("Git"));
    }
    if let Some(path) = std::env::var_os("USERPROFILE") {
        roots.push(
            PathBuf::from(path)
                .join("scoop")
                .join("apps")
                .join("git")
                .join("current"),
        );
    }
    if let Some(path) = std::env::var_os("ProgramData") {
        roots.push(
            PathBuf::from(path)
                .join("scoop")
                .join("apps")
                .join("git")
                .join("current"),
        );
    }
    roots
}

#[cfg(windows)]
fn valid_git_for_windows_root(root: &Path) -> bool {
    root.is_absolute()
        && root.join("bin").join("bash.exe").is_file()
        && (root.join("cmd").join("git.exe").is_file()
            || root.join("bin").join("git.exe").is_file())
}

fn quote_posix(s: &str) -> String {
    if s.is_empty() {
        return "''".into();
    }
    if s.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '/' | '.' | ':' | '=' | ','))
    {
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

fn quote_bun(s: &str) -> String {
    quote_posix(s)
}

fn quote_pwsh(s: &str) -> String {
    if !s.is_empty()
        && s.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '/' | '.' | ':' | '=' | ',')
        })
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
    fn parses_bun() {
        assert_eq!(Shell::parse("bun").unwrap(), Shell::Bun);
    }

    #[test]
    fn bun_invocation_disables_env_files_and_options() {
        let invocation = Shell::Bun.invocation().unwrap();
        assert_eq!(invocation.program, PathBuf::from("bun"));
        assert_eq!(invocation.args, &["--no-env-file", "exec"]);
        assert_eq!(invocation.env_remove, &["BUN_OPTIONS"]);
    }

    #[test]
    fn bun_quoting_has_its_own_posix_compatible_contract() {
        assert_eq!(quote_bun(""), "''");
        assert_eq!(quote_bun("plain/path"), "plain/path");
        assert_eq!(quote_bun("it's ready"), "'it'\\''s ready'");
        assert_eq!(Shell::Bun.quote("two words"), "'two words'");
    }

    #[cfg(not(windows))]
    #[test]
    fn non_windows_bash_uses_path_program_name() {
        let invocation = Shell::Bash.invocation().unwrap();
        assert_eq!(invocation.program, PathBuf::from("bash"));
    }

    #[cfg(windows)]
    mod windows {
        use super::*;
        use std::fs;
        use tempfile::TempDir;

        fn make_git_root(root: &Path, git_under_cmd: bool) {
            fs::create_dir_all(root.join("bin")).unwrap();
            fs::write(root.join("bin").join("bash.exe"), b"").unwrap();
            let git_dir = if git_under_cmd { "cmd" } else { "bin" };
            fs::create_dir_all(root.join(git_dir)).unwrap();
            fs::write(root.join(git_dir).join("git.exe"), b"").unwrap();
        }

        #[test]
        fn validates_bash_and_either_supported_git_location() {
            let temp = TempDir::new().unwrap();
            let cmd_root = temp.path().join("cmd-git");
            let bin_root = temp.path().join("bin-git");
            make_git_root(&cmd_root, true);
            make_git_root(&bin_root, false);

            assert!(valid_git_for_windows_root(&cmd_root));
            assert!(valid_git_for_windows_root(&bin_root));

            fs::remove_file(bin_root.join("bin").join("bash.exe")).unwrap();
            assert!(!valid_git_for_windows_root(&bin_root));
        }

        #[test]
        fn walks_exec_path_ancestors_to_the_git_root() {
            let temp = TempDir::new().unwrap();
            let root = temp.path().join("Git");
            make_git_root(&root, true);
            let exec_path = root.join("mingw64").join("libexec").join("git-core");
            fs::create_dir_all(&exec_path).unwrap();

            assert_eq!(git_root_from_exec_path(&exec_path), Some(root));
        }

        #[test]
        fn rejects_relative_git_roots() {
            assert!(!valid_git_for_windows_root(Path::new("Git")));
        }
    }
}
