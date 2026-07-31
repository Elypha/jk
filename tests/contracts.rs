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
    std::fs::create_dir_all(home).unwrap();
    Command::new(env!("CARGO_BIN_EXE_jk"))
        .current_dir(project.path())
        .env(HOME_VAR, home)
        .env("JK_NO_COLOR", "1")
        .env_remove("JK_CONFIG")
        .env_remove("JK_QUIET")
        .args(args)
        .output()
        .unwrap()
}

fn default_home(project: &TempDir) -> std::path::PathBuf {
    project.path().join("home")
}

#[test]
fn readme_multiline_inline_table_reaches_exact_dry_run_command() {
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
fn local_command_replaces_same_named_global_command() {
    let project = TempDir::new().unwrap();
    let home = default_home(&project);
    std::fs::create_dir_all(home.join(".jk")).unwrap();

    let global = format!("shell = \"{TEST_SHELL}\"\n\n[shared]\ncmd = \"echo global\"\n");
    write(&home.join(".jk").join("config.toml"), &global);

    let local = format!("shell = \"{TEST_SHELL}\"\n\n[shared]\ncmd = \"echo local\"\n");
    write(&project.path().join(".jk"), &local);

    let output = run_jk(&project, &home, &["++dry-run", "shared"]);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "echo local\n");
}
