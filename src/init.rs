use crate::error::{JkError, JkResult};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

#[cfg(windows)]
const TEMPLATE: &str = r##"#:schema https://raw.githubusercontent.com/Elypha/jk/master/schema/jk.schema.json

shell = "pwsh"

[hello]
cmd = "echo hello"
"##;

#[cfg(not(windows))]
const TEMPLATE: &str = r##"#:schema https://raw.githubusercontent.com/Elypha/jk/master/schema/jk.schema.json

shell = "sh"

[hello]
cmd = "echo hello"
"##;

pub fn create_in(cwd: &Path) -> JkResult<PathBuf> {
    let path = cwd.join(".jk");
    let mut file = match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(JkError::InitExists(path.display().to_string()));
        }
        Err(e) => return Err(JkError::Io(e)),
    };
    file.write_all(TEMPLATE.as_bytes())?;
    Ok(path)
}
