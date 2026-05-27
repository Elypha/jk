use std::fmt;

#[derive(Debug)]
pub enum JkError {
    /// `cwd` is the walk-up start; `global_path` is `<home>/.jk/config.toml`
    /// (`None` when `HOME`/`USERPROFILE` is absent). Only raised when both local
    /// and global are absent — global-only mode bypasses this variant.
    ConfigNotFound { cwd: String, global_path: Option<String> },
    ConfigPathInvalid(String),
    ConfigParse(String),
    ConfigSchema(String),
    UnknownFlag(String),
    MalformedFlag { name: String, reason: String },
    UnknownCommand(String),
    MissingArg(String),
    ExtraArgs(usize),
    SpawnFailed(String),
    Io(std::io::Error),
}

impl fmt::Display for JkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JkError::ConfigNotFound { cwd, global_path } => match global_path {
                Some(p) => write!(f, "no .jk found (searched from {}) and no global config at {}", cwd, p),
                None => write!(f, "no .jk found (searched from {}) and no global config", cwd),
            },
            JkError::ConfigPathInvalid(p) => write!(f, "config path invalid: {}", p),
            JkError::ConfigParse(m) => write!(f, "config parse error: {}", m),
            JkError::ConfigSchema(m) => write!(f, "config schema error: {}", m),
            JkError::UnknownFlag(s) => write!(f, "unknown flag: {}", s),
            JkError::MalformedFlag { name, reason } => write!(f, "malformed flag {}: {}", name, reason),
            JkError::UnknownCommand(s) => write!(f, "unknown command: jk {}", s),
            JkError::MissingArg(p) => write!(f, "missing argument for placeholder {}", p),
            JkError::ExtraArgs(n) => write!(f, "{} extra argument(s) with no placeholder to consume them", n),
            JkError::SpawnFailed(s) => write!(f, "failed to spawn {}", s),
            JkError::Io(e) => write!(f, "io error: {}", e),
        }
    }
}

impl std::error::Error for JkError {}

impl From<std::io::Error> for JkError {
    fn from(e: std::io::Error) -> Self {
        JkError::Io(e)
    }
}

pub type JkResult<T> = Result<T, JkError>;
