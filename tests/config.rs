use jk::config::{parse_str, CommandNode};
use jk::shell::Shell;
use serial_test::serial;
use tempfile::TempDir;

const BASIC: &str = include_str!("fixtures/basic.jk");
const TREE: &str = include_str!("fixtures/tree.jk");

#[test]
fn parse_basic() {
    let cfg = parse_str(BASIC).unwrap();

    let build = cfg.tree.lookup(&["build".into()]).unwrap();
    let CommandNode::Leaf(leaf) = build else { panic!("expected leaf") };
    assert_eq!(leaf.desc.as_deref(), Some("build the project"));
    assert_eq!(leaf.cmd_strings(), vec!["cargo build".to_string()]);
    assert_eq!(leaf.shell, Shell::Bash);

    let update = cfg.tree.lookup(&["update".into()]).unwrap();
    let CommandNode::Leaf(leaf) = update else { panic!("expected leaf") };
    assert_eq!(leaf.cmd_strings(), vec!["apt update".to_string(), "apt upgrade #{@}".to_string()]);
}

#[test]
fn parse_tree() {
    let cfg = parse_str(TREE).unwrap();
    let encode = cfg.tree.lookup(&["encode".into()]).unwrap();
    assert!(matches!(encode, CommandNode::Namespace(_)));

    let jpg = cfg.tree.lookup(&["encode".into(), "jpg".into()]).unwrap();
    let CommandNode::Leaf(leaf) = jpg else { panic!("expected leaf") };
    assert_eq!(leaf.desc.as_deref(), Some("encode jpg"));
}

#[test]
fn rejects_empty_cmd_string() {
    let s = r#"
shell = "bash"
[foo]
cmd = ""
"#;
    let err = jk::config::parse_str(s).unwrap_err();
    let jk::error::JkError::ConfigSchema(msg) = err else { panic!() };
    assert!(msg.contains("must not be empty"), "msg: {}", msg);
}

#[test]
fn rejects_empty_cmd_array() {
    let s = r#"
shell = "bash"
[foo]
cmd = []
"#;
    let err = jk::config::parse_str(s).unwrap_err();
    let jk::error::JkError::ConfigSchema(msg) = err else { panic!() };
    assert!(msg.contains("must not be empty"), "msg: {}", msg);
}

#[test]
fn namespace_with_non_table_sibling_errors_clearly() {
    let s = r#"
shell = "bash"
[encode]
quality = 90
[encode.jpg]
cmd = "magick"
"#;
    let err = jk::config::parse_str(s).unwrap_err();
    let jk::error::JkError::ConfigSchema(msg) = err else { panic!() };
    assert!(msg.contains("quality"), "msg should mention the offending field name 'quality': {}", msg);
}

#[test]
fn neither_cmd_nor_children_lists_unknown_fields() {
    let s = r#"
shell = "bash"
[foo]
timeout = 30
"#;
    let err = jk::config::parse_str(s).unwrap_err();
    let jk::error::JkError::ConfigSchema(msg) = err else { panic!() };
    assert!(msg.contains("timeout"), "msg should mention 'timeout': {}", msg);
}

#[test]
fn rejects_namespace_leaf_collision() {
    let s = r#"
shell = "bash"

[encode]
cmd = "magick #{@}"

[encode.jpg]
cmd = "magick"
"#;
    let err = jk::config::parse_str(s).unwrap_err();
    assert!(matches!(err, jk::error::JkError::ConfigSchema(_)));
}

#[test]
fn rejects_empty_node() {
    let s = r#"
[foo]
desc = "no cmd, no children"
"#;
    let err = jk::config::parse_str(s).unwrap_err();
    assert!(matches!(err, jk::error::JkError::ConfigSchema(_)));
}

#[test]
fn rejects_unknown_leaf_field() {
    let s = r#"
[foo]
cmd = "echo hi"
timeout = 30
"#;
    let err = jk::config::parse_str(s).unwrap_err();
    assert!(matches!(err, jk::error::JkError::ConfigSchema(_)));
}

#[test]
#[serial]
fn discover_walks_up() {
    std::env::remove_var("JK_CONFIG");
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let nested = root.join("a/b/c");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(root.join(".jk"), "shell=\"bash\"\n[x]\ncmd=\"echo hi\"\n").unwrap();

    let found = jk::config::discover(&nested, None).unwrap();
    assert_eq!(found, Some(root.join(".jk")));
}

#[test]
#[serial]
fn discover_walk_up_failure_returns_none() {
    std::env::remove_var("JK_CONFIG");
    let tmp = TempDir::new().unwrap();
    let nested = tmp.path().join("a/b");
    std::fs::create_dir_all(&nested).unwrap();
    // If a tempdir ancestor happens to contain a .jk, skip rather than assert falsely.
    let result = jk::config::discover(&nested, None).unwrap();
    if result.is_some() {
        eprintln!("skip: ancestor of tempdir contains .jk: {:?}", result);
        return;
    }
    assert_eq!(result, None);
}

#[test]
#[serial]
fn discover_explicit_path_wins() {
    std::env::remove_var("JK_CONFIG");
    let tmp = TempDir::new().unwrap();
    let target = tmp.path().join("explicit.jk");
    std::fs::write(&target, "shell=\"bash\"\n[x]\ncmd=\"echo\"\n").unwrap();

    let found = jk::config::discover(tmp.path(), Some(target.to_string_lossy().to_string())).unwrap();
    assert_eq!(found, Some(target));
}

#[test]
#[serial]
fn discover_explicit_missing_file_errors() {
    std::env::remove_var("JK_CONFIG");
    let tmp = TempDir::new().unwrap();
    let err = jk::config::discover(tmp.path(), Some("/nonexistent/.jk".into())).unwrap_err();
    assert!(matches!(err, jk::error::JkError::ConfigPathInvalid(_)));
}

#[test]
#[serial]
fn discover_missing_jk_config_env_errors_with_path_invalid() {
    let tmp = TempDir::new().unwrap();
    std::env::set_var("JK_CONFIG", "/no/such/path/.jk");
    let err = jk::config::discover(tmp.path(), None).unwrap_err();
    assert!(
        matches!(err, jk::error::JkError::ConfigPathInvalid(_)),
        "expected ConfigPathInvalid, got: {:?}",
        err
    );
    std::env::remove_var("JK_CONFIG");
}

#[test]
fn rejects_template_with_non_contiguous_placeholders() {
    let s = r#"
shell = "bash"
[bad]
cmd = "echo #{1} #{3}"
"#;
    let err = jk::config::parse_str(s).unwrap_err();
    let jk::error::JkError::ConfigSchema(msg) = err else { panic!() };
    assert!(msg.contains("'bad'.cmd"), "msg should mention path: {}", msg);
}

#[test]
fn rejects_template_with_zero_index() {
    let s = r#"
shell = "bash"
[bad]
cmd = "echo #{0}"
"#;
    let err = jk::config::parse_str(s).unwrap_err();
    assert!(matches!(err, jk::error::JkError::ConfigSchema(_)));
}

#[test]
fn rejects_template_with_unknown_placeholder_form() {
    let s = r#"
shell = "bash"
[bad]
cmd = "echo #{abc}"
"#;
    let err = jk::config::parse_str(s).unwrap_err();
    assert!(matches!(err, jk::error::JkError::ConfigSchema(_)));
}

#[test]
fn accepts_template_with_raw_placeholders() {
    let s = r#"
shell = "bash"
[ok]
cmd = "ffmpeg -vf '#{1!}' #{@!}"
"#;
    let cfg = jk::config::parse_str(s).unwrap();
    assert!(cfg.tree.lookup(&["ok".into()]).is_some());
}

#[test]
fn accepts_template_with_literal_braces() {
    // Literal `{...}`, `${VAR}`, `{{X}}`, awk actions, brace expansions — all pass through.
    let s = r#"
shell = "bash"
[awk]
cmd = "awk '{print $1}' #{1}"
[var]
cmd = "echo ${HOME} #{1}"
[double]
cmd = "helm template chart --set v={{X}}"
[expansion]
cmd = "echo {1..10}"
"#;
    let cfg = jk::config::parse_str(s).unwrap();
    assert!(cfg.tree.lookup(&["awk".into()]).is_some());
    assert!(cfg.tree.lookup(&["var".into()]).is_some());
    assert!(cfg.tree.lookup(&["double".into()]).is_some());
    assert!(cfg.tree.lookup(&["expansion".into()]).is_some());
}

#[test]
fn rejects_non_contiguous_at_leaf_aggregate() {
    let s = r#"
shell = "bash"
[seq]
cmd = ["echo #{1}", "echo #{1} #{3}"]
"#;
    let err = jk::config::parse_str(s).unwrap_err();
    let jk::error::JkError::ConfigSchema(msg) = err else { panic!() };
    assert!(msg.contains("'seq'.cmd"), "msg should mention leaf path: {}", msg);
    assert!(
        msg.contains("missing") || msg.contains("non-contiguous"),
        "msg should explain the gap: {}", msg
    );
}

#[test]
fn accepts_seq_with_split_indices_across_items() {
    // Each item looks incomplete, but the leaf-level union {1, 2} is contiguous.
    let s = r#"
shell = "bash"
[seq]
cmd = ["echo #{1}", "echo #{2}"]
"#;
    let cfg = jk::config::parse_str(s).unwrap();
    assert!(cfg.tree.lookup(&["seq".into()]).is_some());
}

#[test]
fn rejects_unsupported_shell_at_file_level() {
    let s = r#"
shell = "csh"
[hello]
cmd = "echo hi"
"#;
    let err = jk::config::parse_str(s).unwrap_err();
    let jk::error::JkError::ConfigSchema(msg) = err else { panic!() };
    assert!(msg.contains("csh"), "msg should mention bad shell name: {}", msg);
}

#[test]
fn rejects_unsupported_shell_at_leaf_level() {
    let s = r#"
shell = "bash"
[hello]
shell = "csh"
cmd = "echo hi"
"#;
    let err = jk::config::parse_str(s).unwrap_err();
    let jk::error::JkError::ConfigSchema(msg) = err else { panic!() };
    assert!(msg.contains("'hello'.shell"), "msg should pinpoint the leaf: {}", msg);
    assert!(msg.contains("csh"), "msg should mention bad shell name: {}", msg);
}

#[test]
#[serial]
fn discover_uses_jk_config_env() {
    let tmp = TempDir::new().unwrap();
    let target = tmp.path().join("from-env.jk");
    std::fs::write(&target, "shell=\"bash\"\n[x]\ncmd=\"echo\"\n").unwrap();
    std::env::set_var("JK_CONFIG", target.to_string_lossy().to_string());

    let other_tmp = TempDir::new().unwrap();
    let found = jk::config::discover(other_tmp.path(), None).unwrap();
    assert_eq!(found, Some(target));

    std::env::remove_var("JK_CONFIG");
}

#[test]
#[serial]
fn discover_treats_empty_jk_config_as_unset() {
    std::env::set_var("JK_CONFIG", "");
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    std::fs::write(root.join(".jk"), "shell=\"bash\"\n[x]\ncmd=\"echo\"\n").unwrap();

    // Should fall through env branch and find via walk-up.
    let found = jk::config::discover(root, None).unwrap();
    assert_eq!(found, Some(root.join(".jk")));

    std::env::remove_var("JK_CONFIG");
}

#[test]
fn merge_local_overrides_global() {
    let global = jk::config::parse_str(r#"
shell = "bash"
[build]
cmd = "global build"
[clean]
cmd = "rm -rf out"
"#).unwrap();

    let local = jk::config::parse_str(r#"
shell = "pwsh"
[build]
cmd = "local build"
[deploy]
cmd = "scp ..."
"#).unwrap();

    let merged = jk::config::merge(global, local).unwrap();
    let build = merged.tree.lookup(&["build".into()]).unwrap();
    let jk::config::CommandNode::Leaf(leaf) = build else { panic!() };
    assert_eq!(leaf.cmd_strings(), vec!["local build".to_string()]);

    assert!(merged.tree.lookup(&["clean".into()]).is_some());
    assert!(merged.tree.lookup(&["deploy".into()]).is_some());
}

#[test]
fn merge_namespace_leaf_conflict_errors() {
    let global = jk::config::parse_str(r#"
shell = "bash"
[encode]
cmd = "..."
"#).unwrap();
    let local = jk::config::parse_str(r#"
shell = "bash"
[encode.jpg]
cmd = "..."
"#).unwrap();

    let err = jk::config::merge(global, local).unwrap_err();
    assert!(matches!(err, jk::error::JkError::ConfigSchema(_)));
}

#[test]
fn merge_tags_origins() {
    use jk::config::{tag_all_origins, Origin};

    let mut global = jk::config::parse_str(r#"
shell = "bash"
[from-global]
cmd = "echo g"
[both]
cmd = "global both"
"#).unwrap();
    tag_all_origins(&mut global, Origin::GlobalOnly);

    let local = jk::config::parse_str(r#"
shell = "bash"
[from-local]
cmd = "echo l"
[both]
cmd = "local both"
"#).unwrap();

    let merged = jk::config::merge(global, local).unwrap();

    let g = merged.tree.lookup(&["from-global".into()]).unwrap();
    let jk::config::CommandNode::Leaf(g_leaf) = g else { panic!() };
    assert_eq!(g_leaf.origin, Origin::GlobalOnly);

    let l = merged.tree.lookup(&["from-local".into()]).unwrap();
    let jk::config::CommandNode::Leaf(l_leaf) = l else { panic!() };
    assert_eq!(l_leaf.origin, Origin::LocalOnly);

    let both = merged.tree.lookup(&["both".into()]).unwrap();
    let jk::config::CommandNode::Leaf(both_leaf) = both else { panic!() };
    assert_eq!(both_leaf.origin, Origin::Override);
    assert_eq!(both_leaf.cmd_strings(), vec!["local both".to_string()]);
}

#[test]
fn merge_preserves_per_file_shell() {
    let global = jk::config::parse_str(r#"
shell = "bash"
[from-global]
cmd = "uname"
"#).unwrap();
    let local = jk::config::parse_str(r#"
shell = "pwsh"
[from-local]
cmd = "Get-Date"
"#).unwrap();

    let merged = jk::config::merge(global, local).unwrap();

    let g = merged.tree.lookup(&["from-global".into()]).unwrap();
    let jk::config::CommandNode::Leaf(g_leaf) = g else { panic!() };
    assert_eq!(g_leaf.shell, Shell::Bash);

    let l = merged.tree.lookup(&["from-local".into()]).unwrap();
    let jk::config::CommandNode::Leaf(l_leaf) = l else { panic!() };
    assert_eq!(l_leaf.shell, Shell::Pwsh);
}
