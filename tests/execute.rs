use jk::execute::{run_one, run_sequence};
use jk::output::Out;
use jk::shell::Shell;

#[test]
fn run_simple_bash_command() {
    if which::which("bash").is_err() {
        eprintln!("skip: bash not on PATH");
        return;
    }
    let exit = run_one("true", Shell::Bash).unwrap();
    assert_eq!(exit, 0);
}

#[test]
fn run_failing_command_returns_nonzero() {
    if which::which("bash").is_err() {
        eprintln!("skip: bash not on PATH");
        return;
    }
    let exit = run_one("false", Shell::Bash).unwrap();
    assert_ne!(exit, 0);
}

#[test]
fn sequence_fail_fast() {
    if which::which("bash").is_err() { eprintln!("skip"); return; }
    let out = Out::from_env();
    let exit = run_sequence(&["true".into(), "false".into(), "echo should-not-run".into()], Shell::Bash, &out, "test").unwrap();
    assert_ne!(exit, 0);
}

#[test]
fn sequence_all_pass() {
    if which::which("bash").is_err() { eprintln!("skip"); return; }
    let out = Out::from_env();
    let exit = run_sequence(&["true".into(), "echo ok".into()], Shell::Bash, &out, "test").unwrap();
    assert_eq!(exit, 0);
}

#[test]
fn explicit_ansi_bytes_pass_through() {
    if which::which("bash").is_err() {
        eprintln!("skip: bash not on PATH");
        return;
    }
    use assert_cmd::Command;
    use predicates::prelude::*;
    use tempfile::TempDir;

    let tmp = TempDir::new().unwrap();
    std::fs::write(
        tmp.path().join(".jk"),
        r#"
shell = "bash"
[red]
cmd = "printf '\\033[31mhello\\033[0m'"
"#,
    )
    .unwrap();

    Command::cargo_bin("jk")
        .unwrap()
        .current_dir(tmp.path())
        .arg("red")
        .assert()
        .success()
        .stdout(predicate::str::contains("\x1b[31m"));
}

#[test]
fn child_sees_pipe_when_jk_stdout_piped() {
    if which::which("bash").is_err() {
        eprintln!("skip: bash not on PATH");
        return;
    }
    use assert_cmd::Command;
    use predicates::prelude::*;
    use tempfile::TempDir;

    let tmp = TempDir::new().unwrap();
    std::fs::write(
        tmp.path().join(".jk"),
        r#"
shell = "bash"
[probe]
cmd = "[ -t 1 ] && echo TTY || echo PIPE"
"#,
    )
    .unwrap();

    Command::cargo_bin("jk")
        .unwrap()
        .current_dir(tmp.path())
        .arg("probe")
        .assert()
        .success()
        .stdout(predicate::str::contains("PIPE"));
}
