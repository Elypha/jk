//! Signal-propagation invariants. Unix-only because Windows uses console
//! control events instead of POSIX signals + process groups, and jk's spec
//! pins these guarantees to OS default fg-pgrp behavior on Unix.

#![cfg(unix)]

use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

fn wait_with_timeout(child: &mut Child, timeout: Duration) -> Option<ExitStatus> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) => {}
            Err(e) => panic!("try_wait failed: {e}"),
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn poll_pid_file(path: &std::path::Path, timeout: Duration) -> Option<i32> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(s) = std::fs::read_to_string(path) {
            if let Some(pid) = s.trim().parse::<i32>().ok().filter(|p| *p > 0) {
                return Some(pid);
            }
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// Sending SIGINT to jk's process group must:
///   1. propagate to the child shell via OS default fg-pgrp delivery
///      (jk installs no SIGINT handler), and
///   2. surface to jk's caller as exit code 130 (`128 + SIGINT`).
///
/// Why `kill(-pgrp)` and not `kill(jk_pid)`: signaling jk alone only proves
/// jk dies; the invariant under test is that the *child* receives the same
/// signal through pgrp membership — that is what terminal Ctrl+C does.
#[test]
fn ctrl_c_propagates_to_child_via_process_group() {
    if which::which("bash").is_err() {
        eprintln!("skip: bash not on PATH");
        return;
    }

    let tmp = tempfile::TempDir::new().unwrap();
    let pidfile = tmp.path().join("child.pid");
    let jk_config = format!(
        r#"
shell = "bash"
[long]
cmd = "echo $$ > '{pid}'; sleep 30"
"#,
        pid = pidfile.to_str().unwrap()
    );
    std::fs::write(tmp.path().join(".jk"), jk_config).unwrap();

    let bin = env!("CARGO_BIN_EXE_jk");
    let mut child = Command::new(bin)
        .current_dir(tmp.path())
        .arg("long")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .expect("spawn jk");

    let jk_pid = child.id() as i32;

    let child_pid = poll_pid_file(&pidfile, Duration::from_secs(5))
        .expect("child shell never wrote its PID");

    let rc = unsafe { libc::kill(-jk_pid, libc::SIGINT) };
    assert_eq!(rc, 0, "kill(-{jk_pid}, SIGINT) failed: {}", std::io::Error::last_os_error());

    let status = wait_with_timeout(&mut child, Duration::from_secs(5)).unwrap_or_else(|| {
        let _ = child.kill();
        panic!("jk did not exit within 5s after SIGINT");
    });

    let surfaced = status
        .code()
        .unwrap_or_else(|| 128 + status.signal().expect("no exit code and no signal"));
    assert_eq!(
        surfaced, 130,
        "expected jk to surface SIGINT as 130, got {surfaced} (raw status: {status:?})"
    );

    // After jk has exited, its child shell should have been reaped (SIGINT
    // delivered to the same pgrp killed it; jk's own waitpid before dying
    // collected the status). kill(pid, 0) returns -1/ESRCH for a dead PID.
    let alive = unsafe { libc::kill(child_pid, 0) };
    assert_eq!(
        alive, -1,
        "child PID {child_pid} still reachable after jk exited"
    );
}
