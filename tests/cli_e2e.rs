use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn write_jk(tmp: &TempDir, contents: &str) {
    std::fs::write(tmp.path().join(".jk"), contents).unwrap();
}

#[test]
fn version_flag_prints_and_exits_zero() {
    Command::cargo_bin("jk")
        .unwrap()
        .arg("++version")
        .assert()
        .success()
        .stdout(predicate::str::starts_with("jk "));
}

#[test]
fn unknown_flag_errors() {
    Command::cargo_bin("jk")
        .unwrap()
        .arg("++nope")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown flag"));
}

#[test]
fn list_top_level_when_no_command() {
    let tmp = TempDir::new().unwrap();
    write_jk(&tmp, r#"
shell = "bash"
[hello]
desc = "say hi"
cmd = "echo hi"
"#);
    Command::cargo_bin("jk")
        .unwrap()
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("hello"));
}

#[test]
fn list_namespace_children() {
    let tmp = TempDir::new().unwrap();
    write_jk(&tmp, r#"
shell = "bash"
[encode.jpg]
desc = "encode jpg"
cmd = "magick #{1}"
[encode.webp]
desc = "encode webp"
cmd = "cwebp #{@}"
"#);
    Command::cargo_bin("jk")
        .unwrap()
        .current_dir(tmp.path())
        .arg("encode")
        .assert()
        .success()
        .stdout(predicate::str::contains("jpg"))
        .stdout(predicate::str::contains("webp"));
}

#[test]
fn dry_run_prints_rendered_command() {
    let tmp = TempDir::new().unwrap();
    write_jk(&tmp, r#"
shell = "bash"
[encode]
cmd = "magick #{1} #{2}"
"#);
    Command::cargo_bin("jk")
        .unwrap()
        .current_dir(tmp.path())
        .args(["++dry-run", "encode", "in.png", "out.jpg"])
        .assert()
        .success()
        .stdout(predicate::str::contains("magick in.png out.jpg"));
}

#[test]
fn unknown_command_errors() {
    let tmp = TempDir::new().unwrap();
    write_jk(&tmp, r#"
shell = "bash"
[hello]
cmd = "echo hi"
"#);
    Command::cargo_bin("jk")
        .unwrap()
        .current_dir(tmp.path())
        .arg("nonexistent")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown command"));
}

#[test]
fn missing_arg_errors() {
    let tmp = TempDir::new().unwrap();
    write_jk(&tmp, r#"
shell = "bash"
[encode]
cmd = "magick #{1} #{2}"
"#);
    Command::cargo_bin("jk")
        .unwrap()
        .current_dir(tmp.path())
        .args(["++dry-run", "encode", "only-one"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("missing argument"));
}

#[test]
fn missing_shell_errors_at_config_load() {
    let tmp = TempDir::new().unwrap();
    write_jk(&tmp, r#"
[hello]
cmd = "echo hi"
"#);
    Command::cargo_bin("jk")
        .unwrap()
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("no shell declared"));
}

#[test]
fn exit_code_passthrough() {
    if which::which("bash").is_err() { eprintln!("skip: bash not on PATH"); return; }
    let tmp = TempDir::new().unwrap();
    write_jk(&tmp, r#"
shell = "bash"
[fail42]
cmd = "exit 42"
"#);
    Command::cargo_bin("jk")
        .unwrap()
        .current_dir(tmp.path())
        .arg("fail42")
        .assert()
        .code(42);
}

#[test]
fn sequence_fail_fast_returns_failed_step_code() {
    if which::which("bash").is_err() { eprintln!("skip: bash not on PATH"); return; }
    let tmp = TempDir::new().unwrap();
    write_jk(&tmp, r#"
shell = "bash"
[seq]
cmd = ["true", "exit 7", "echo should-not-run"]
"#);
    Command::cargo_bin("jk")
        .unwrap()
        .current_dir(tmp.path())
        .arg("seq")
        .assert()
        .code(7);
}

#[test]
fn jk_quiet_suppresses_listing_header_only() {
    let tmp = TempDir::new().unwrap();
    write_jk(&tmp, r#"
shell = "pwsh"
[hello]
desc = "say hi"
cmd = "echo hi"
"#);
    Command::cargo_bin("jk")
        .unwrap()
        .current_dir(tmp.path())
        .env("JK_QUIET", "1")
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("[jk] configs:").not())
        .stdout(predicate::str::contains("hello"));
}

#[test]
fn list_top_level_includes_configs_header() {
    let tmp = TempDir::new().unwrap();
    write_jk(&tmp, r#"
shell = "bash"
[hello]
desc = "say hi"
cmd = "echo hi"
"#);
    Command::cargo_bin("jk")
        .unwrap()
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("[jk] configs:"))
        .stdout(predicate::str::contains("global:"))
        .stdout(predicate::str::contains("local:"))
        .stdout(predicate::str::contains("hello"));
}

#[test]
fn list_namespace_does_not_repeat_configs_header() {
    let tmp = TempDir::new().unwrap();
    write_jk(&tmp, r#"
shell = "bash"
[encode.jpg]
desc = "encode jpg"
cmd = "magick #{1}"
"#);
    Command::cargo_bin("jk")
        .unwrap()
        .current_dir(tmp.path())
        .arg("encode")
        .assert()
        .success()
        .stdout(predicate::str::contains("[jk] configs:").not())
        .stdout(predicate::str::contains("jpg"));
}

#[test]
fn jk_quiet_only_activates_on_exact_one() {
    let tmp = TempDir::new().unwrap();
    write_jk(&tmp, r#"
shell = "bash"
[hello]
desc = "say hi"
cmd = "echo hi"
"#);
    Command::cargo_bin("jk")
        .unwrap()
        .current_dir(tmp.path())
        .env("JK_QUIET", "0")
        .assert()
        .success()
        .stdout(predicate::str::contains("hello"));

    Command::cargo_bin("jk")
        .unwrap()
        .current_dir(tmp.path())
        .env("JK_QUIET", "true")
        .assert()
        .success()
        .stdout(predicate::str::contains("hello"));
}

#[test]
fn double_dash_separator_passes_through_to_command() {
    let tmp = TempDir::new().unwrap();
    write_jk(&tmp, r#"
shell = "bash"
[show]
cmd = "echo #{1} #{2}"
"#);
    Command::cargo_bin("jk")
        .unwrap()
        .current_dir(tmp.path())
        .args(["++dry-run", "show", "--", "++version", "++bogus"])
        .assert()
        .success()
        .stdout(predicate::str::contains("'++version'"))
        .stdout(predicate::str::contains("'++bogus'"));
}

#[test]
fn version_does_not_bypass_unknown_flag_validation() {
    Command::cargo_bin("jk")
        .unwrap()
        .args(["++version", "++bogus"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown flag"));
}

// Windows-only: pwsh is typically absent on Linux/macOS.
#[cfg(target_os = "windows")]
#[test]
fn pwsh_passthrough_and_exit_code() {
    if which::which("pwsh").is_err() {
        eprintln!("skip: pwsh not on PATH");
        return;
    }
    let tmp = TempDir::new().unwrap();
    write_jk(&tmp, r#"
shell = "pwsh"
[ok]
cmd = "Write-Output ok"
[fail7]
cmd = "exit 7"
"#);
    Command::cargo_bin("jk")
        .unwrap()
        .current_dir(tmp.path())
        .arg("ok")
        .assert()
        .success()
        .stdout(predicate::str::contains("ok"));

    Command::cargo_bin("jk")
        .unwrap()
        .current_dir(tmp.path())
        .arg("fail7")
        .assert()
        .code(7);
}

#[test]
fn dry_run_release_sequence_with_mixed_items() {
    let tmp = TempDir::new().unwrap();
    write_jk(&tmp, r#"
shell = "bash"
[release]
cmd = ["jk clean", "jk build", "jk package #{1}"]
[clean]
cmd = "echo cleaned"
[build]
cmd = "echo built"
[package]
cmd = "echo packaged #{1}"
"#);
    Command::cargo_bin("jk")
        .unwrap()
        .current_dir(tmp.path())
        .args(["++dry-run", "release", "v1.5"])
        .assert()
        .success()
        .stdout(predicate::str::contains("jk clean"))
        .stdout(predicate::str::contains("jk build"))
        .stdout(predicate::str::contains("jk package v1.5"));
}

#[test]
fn dry_run_update_sequence_with_at_in_only_one_item() {
    let tmp = TempDir::new().unwrap();
    write_jk(&tmp, r#"
shell = "bash"
[update]
cmd = ["apt update", "apt upgrade #{@}"]
"#);
    Command::cargo_bin("jk")
        .unwrap()
        .current_dir(tmp.path())
        .args(["++dry-run", "update", "-y"])
        .assert()
        .success()
        .stdout(predicate::str::contains("apt update"))
        .stdout(predicate::str::contains("apt upgrade -y"));
}

#[test]
fn dry_run_release_too_many_args_errors() {
    let tmp = TempDir::new().unwrap();
    write_jk(&tmp, r#"
shell = "bash"
[release]
cmd = ["jk clean", "jk build", "jk package #{1}"]
[clean]
cmd = "echo cleaned"
[build]
cmd = "echo built"
[package]
cmd = "echo packaged #{1}"
"#);
    Command::cargo_bin("jk")
        .unwrap()
        .current_dir(tmp.path())
        .args(["++dry-run", "release", "v1.5", "extra"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("extra argument"));
}

#[test]
fn event_lines_carry_local_timestamp() {
    if which::which("bash").is_err() { eprintln!("skip: bash not on PATH"); return; }
    let tmp = TempDir::new().unwrap();
    write_jk(&tmp, r#"
shell = "bash"
[hello]
cmd = "true"
"#);
    let assert = Command::cargo_bin("jk")
        .unwrap()
        .current_dir(tmp.path())
        .arg("hello")
        .assert()
        .success();

    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).to_string();
    let ts_pat = predicate::str::is_match(r"\[jk\]\[\d{2}:\d{2}:\d{2}\.\d{3}\] running hello").unwrap();
    let comp_pat = predicate::str::is_match(r"\[jk\]\[\d{2}:\d{2}:\d{2}\.\d{3}\] completed in \d+ms").unwrap();
    assert!(ts_pat.eval(stderr.as_str()), "stderr missing running line: {stderr}");
    assert!(comp_pat.eval(stderr.as_str()), "stderr missing completed line: {stderr}");
}

#[test]
fn explicit_config_path_invalid_error_message() {
    // B1: explicit ++config to nonexistent file should NOT say "searched from".
    Command::cargo_bin("jk")
        .unwrap()
        .args(["++config=/no/such/file.jk", "anything"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("config path invalid"))
        .stderr(predicate::str::contains("searched from").not());
}
