//! Global-config e2e tests. Each test redirects `HOME` / `USERPROFILE` on the
//! spawned `jk` process only — no global state mutation, no `serial_test` needed.

use assert_cmd::Command;
use std::path::PathBuf;
use tempfile::TempDir;

#[cfg(unix)]
const HOME_VAR: &str = "HOME";
#[cfg(windows)]
const HOME_VAR: &str = "USERPROFILE";

/// Returns the expected global-config path under a given home root, and
/// pre-creates the `.jk/` parent directory so callers can write into it.
fn redirect_global_config_root(home: &std::path::Path) -> PathBuf {
    let parent = home.join(".jk");
    std::fs::create_dir_all(&parent).unwrap();
    parent.join("config.toml")
}

fn jk_with_global(
    cwd: &std::path::Path,
    home: &std::path::Path,
) -> Command {
    let mut cmd = Command::cargo_bin("jk").unwrap();
    cmd.current_dir(cwd)
        .env(HOME_VAR, home)
        .env("JK_NO_COLOR", "1")
        .env_remove("JK_QUIET")
        .env_remove("JK_CONFIG");
    cmd
}

#[test]
fn root_listing_merges_global_and_local() {
    let project = TempDir::new().unwrap();
    let global_root = TempDir::new().unwrap();

    let global_cfg_path = redirect_global_config_root(global_root.path());
    std::fs::write(
        &global_cfg_path,
        r#"
shell = "bash"
[gonly]
desc = "from global"
cmd = "echo g"
[shared]
desc = "global version"
cmd = "echo g-shared"
"#,
    )
    .unwrap();

    std::fs::write(
        project.path().join(".jk"),
        r#"
shell = "bash"
[lonly]
desc = "from local"
cmd = "echo l"
[shared]
desc = "local override"
cmd = "echo l-shared"
"#,
    )
    .unwrap();

    let assert = jk_with_global(project.path(), global_root.path())
        .assert()
        .success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();

    assert!(stdout.contains("[jk] configs:"), "missing header marker:\n{stdout}");
    assert!(stdout.contains("global: "), "missing global header line:\n{stdout}");
    assert!(stdout.contains("local:  "), "missing local header line:\n{stdout}");
    assert!(
        !stdout.contains("local:  (none)"),
        "local should not be (none) when project has .jk:\n{stdout}"
    );

    assert!(stdout.contains("(g) gonly"), "global-only leaf missing (g) marker:\n{stdout}");
    assert!(stdout.contains("(o) shared"), "overridden leaf missing (o) marker:\n{stdout}");
    assert!(stdout.contains("from local"), "local-only desc missing:\n{stdout}");
}

#[test]
fn root_listing_global_only_when_no_local() {
    let project = TempDir::new().unwrap();
    let global_root = TempDir::new().unwrap();

    let global_cfg_path = redirect_global_config_root(global_root.path());
    std::fs::write(
        &global_cfg_path,
        r#"
shell = "bash"
[only-global]
desc = "from global"
cmd = "echo g"
"#,
    )
    .unwrap();
    let assert = jk_with_global(project.path(), global_root.path())
        .assert()
        .success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();

    assert!(
        stdout.contains("local:  (none)"),
        "expected local: (none) header in global-only mode:\n{stdout}"
    );
    assert!(
        stdout.contains("(g) only-global"),
        "global-only leaf missing or unmarked:\n{stdout}"
    );
}

#[test]
fn jk_config_env_overrides_cwd_jk() {
    let cwd = TempDir::new().unwrap();
    let alt = TempDir::new().unwrap();
    let global_root = TempDir::new().unwrap();

    std::fs::write(
        cwd.path().join(".jk"),
        r#"
shell = "bash"
[from-cwd]
cmd = "echo cwd"
"#,
    )
    .unwrap();

    let alt_cfg = alt.path().join("alt.jk");
    std::fs::write(
        &alt_cfg,
        r#"
shell = "bash"
[from-env]
cmd = "echo env"
"#,
    )
    .unwrap();

    let mut cmd = jk_with_global(cwd.path(), global_root.path());
    cmd.env("JK_CONFIG", &alt_cfg);

    let assert = cmd.assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();

    assert!(stdout.contains("from-env"), "JK_CONFIG target not loaded:\n{stdout}");
    assert!(
        !stdout.contains("from-cwd"),
        "cwd .jk leaked through despite JK_CONFIG:\n{stdout}"
    );
}
