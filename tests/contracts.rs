use std::path::Path;
use std::process::{Command, Output};
use tempfile::TempDir;

#[cfg(unix)]
const HOME_VAR: &str = "HOME";
#[cfg(windows)]
const HOME_VAR: &str = "USERPROFILE";

#[cfg(unix)]
const TEST_SHELL: &str = "sh";
#[cfg(windows)]
const TEST_SHELL: &str = "pwsh";

fn write(path: &Path, contents: &str) {
    std::fs::write(path, contents).unwrap();
}

fn run_jk(project: &TempDir, home: &Path, args: &[&str]) -> Output {
    run_jk_with_env(project, home, args, &[])
}

fn run_jk_with_env(project: &TempDir, home: &Path, args: &[&str], env: &[(&str, &str)]) -> Output {
    std::fs::create_dir_all(home).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_jk"));
    command
        .current_dir(project.path())
        .env(HOME_VAR, home)
        .env("JK_NO_COLOR", "1")
        .env_remove("JK_CONFIG")
        .env_remove("JK_HOME")
        .env_remove("JK_QUIET")
        .args(args);
    for (key, value) in env {
        command.env(key, value);
    }
    command.output().unwrap()
}

fn default_home(project: &TempDir) -> std::path::PathBuf {
    project.path().join("home")
}

#[test]
fn toml_1_1_multiline_inline_table_reaches_exact_dry_run_command() {
    let project = TempDir::new().unwrap();
    let config = r#"
shell = "__SHELL__"

[encode]

jpg = {
  desc = "encode JPG",
  cmd = '''
    magick #{1}
      -quality 90
      #{2}
  ''',
}
"#
    .replace("__SHELL__", TEST_SHELL);
    let config_path = project.path().join(".jk");
    write(&config_path, &config);

    let config_arg = format!("++config={}", config_path.display());
    let output = run_jk(
        &project,
        &default_home(&project),
        &[
            &config_arg,
            "++dry-run",
            "encode",
            "jpg",
            "input file.tiff",
            "output file.jpg",
        ],
    );

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "magick 'input file.tiff' -quality 90 'output file.jpg'\n"
    );
}

#[test]
fn child_exit_code_is_process_exit_code() {
    let project = TempDir::new().unwrap();
    let config = format!("shell = \"{TEST_SHELL}\"\n\n[fail]\ncmd = \"exit 42\"\n");
    write(&project.path().join(".jk"), &config);

    let output = run_jk(&project, &default_home(&project), &["fail"]);

    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn failed_sequence_step_returns_its_code_and_does_not_run_later_steps() {
    let project = TempDir::new().unwrap();
    let marker = project.path().join("should-not-run");

    #[cfg(unix)]
    let marker_command = format!("touch '{}'", marker.display());
    #[cfg(windows)]
    let marker_command = format!(
        "New-Item -ItemType File -LiteralPath '{}' -Force",
        marker.display()
    );

    let config = format!(
        "shell = \"{TEST_SHELL}\"\n\n[sequence]\ncmd = [\n  \"exit 7\",\n  '''{marker_command}''',\n]\n"
    );
    write(&project.path().join(".jk"), &config);

    let output = run_jk(&project, &default_home(&project), &["sequence"]);

    assert_eq!(output.status.code(), Some(7));
    assert!(!marker.exists(), "a step after the failure was executed");
}

#[test]
fn bun_shell_round_trips_quoted_placeholders() {
    let project = TempDir::new().unwrap();
    write(
        &project.path().join(".jk"),
        r#"
shell = "bun"

[args]
cmd = "bun -e 'console.log(JSON.stringify(process.argv.slice(1)))' -- #{@}"
"#,
    );

    let output = run_jk(
        &project,
        &default_home(&project),
        &[
            "args",
            "",
            "two words",
            "a'b",
            "$HOME",
            r"C:\Program Files\x",
            "雪",
        ],
    );

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "[\"\",\"two words\",\"a'b\",\"$HOME\",\"C:\\\\Program Files\\\\x\",\"雪\"]\n"
    );
}

#[test]
fn bun_shell_ignores_dotenv_and_bun_options() {
    let project = TempDir::new().unwrap();
    write(&project.path().join(".env"), "JK_BUN_PROBE=loaded\n");
    write(
        &project.path().join(".jk"),
        r#"
shell = "bun"

[isolated]
cmd = '''if [ -z "$JK_BUN_PROBE" ]; then exit 0; else exit 91; fi'''
"#,
    );

    let output = run_jk_with_env(
        &project,
        &default_home(&project),
        &["isolated"],
        &[("BUN_OPTIONS", r"--cwd=Z:\definitely-missing")],
    );

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn bun_shell_exit_code_is_process_exit_code() {
    let project = TempDir::new().unwrap();
    write(
        &project.path().join(".jk"),
        "shell = \"bun\"\n\n[fail]\ncmd = \"exit 42\"\n",
    );

    let output = run_jk(&project, &default_home(&project), &["fail"]);

    assert_eq!(output.status.code(), Some(42));
}

#[cfg(windows)]
#[test]
fn windows_bash_uses_git_for_windows_bash() {
    let project = TempDir::new().unwrap();
    write(
        &project.path().join(".jk"),
        r#"
shell = "bash"

[version]
cmd = '''printf 'git-for-windows:%s' "$BASH_VERSION"'''
"#,
    );

    let output = run_jk(&project, &default_home(&project), &["version"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout
            .strip_prefix("git-for-windows:")
            .is_some_and(|version| !version.is_empty()),
        "the Windows Bash backend did not expose a GNU Bash version"
    );
}
