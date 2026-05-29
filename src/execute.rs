use crate::error::{JkError, JkResult};
use crate::output::Out;
use crate::shell::Shell;

pub fn run_one(rendered_cmd: &str, shell: Shell) -> JkResult<i32> {
    let inv = shell.invocation();
    let mut cmd = std::process::Command::new(inv.program);
    cmd.args(inv.args);
    cmd.arg(rendered_cmd);
    for k in inv.env_remove {
        cmd.env_remove(k);
    }
    let status = cmd
        .status()
        .map_err(|e| JkError::SpawnFailed(format!("'{}': {}", inv.program, e)))?;
    Ok(exit_code_from_status(status))
}

/// On POSIX, `ExitStatus::code()` returns `None` when the child was killed by a
/// signal. Following the `bash`/`zsh`/`make`/`timeout` convention, return
/// `128 + signum` in that case (e.g. SIGINT → 130, SIGKILL → 137).
/// On Windows, `code()` never returns `None`.
fn exit_code_from_status(status: std::process::ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        return code;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(sig) = status.signal() {
            return 128 + sig;
        }
    }
    -1
}

pub fn run_sequence(rendered_cmds: &[String], shell: Shell, out: &Out) -> JkResult<i32> {
    let start = std::time::Instant::now();

    for (idx, cmd) in rendered_cmds.iter().enumerate() {
        out.step_header(cmd);
        let exit = run_one(cmd, shell)?;
        if exit != 0 {
            out.failed(idx, exit);
            return Ok(exit);
        }
    }

    out.completed(start.elapsed().as_millis());
    Ok(0)
}
