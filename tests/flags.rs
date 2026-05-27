use jk::cli::parse_argv;

#[test]
fn no_flags() {
    let r = parse_argv(vec!["build".into(), "foo".into()]).unwrap();
    assert_eq!(r.path, vec!["build", "foo"]);
    assert!(!r.dry_run);
    assert!(!r.version);
    assert_eq!(r.config_path, None);
}

#[test]
fn dry_run_flag_anywhere() {
    let cases = vec![
        vec!["++dry-run", "build", "foo"],
        vec!["build", "++dry-run", "foo"],
        vec!["build", "foo", "++dry-run"],
    ];
    for c in cases {
        let r = parse_argv(c.iter().map(|s| s.to_string()).collect()).unwrap();
        assert!(r.dry_run);
        assert_eq!(r.path, vec!["build", "foo"]);
    }
}

#[test]
fn version_flag() {
    let r = parse_argv(vec!["++version".into()]).unwrap();
    assert!(r.version);
}

#[test]
fn config_with_value() {
    let r = parse_argv(vec!["++config=/tmp/.jk".into(), "build".into()]).unwrap();
    assert_eq!(r.config_path, Some("/tmp/.jk".into()));
    assert_eq!(r.path, vec!["build"]);
}

#[test]
fn unknown_flag_errors() {
    let err = parse_argv(vec!["++foo".into()]).unwrap_err();
    assert!(matches!(err, jk::error::JkError::UnknownFlag(_)));
}

#[test]
fn space_separated_config_value_is_malformed() {
    // `++config` without `=` is malformed (known flag, missing required value)
    let err = parse_argv(vec!["++config".into(), "/tmp/.jk".into()]).unwrap_err();
    let jk::error::JkError::MalformedFlag { name, .. } = err else {
        panic!("expected MalformedFlag");
    };
    assert_eq!(name, "++config");
}

#[test]
fn empty_config_value_is_malformed() {
    let err = parse_argv(vec!["++config=".into(), "build".into()]).unwrap_err();
    let jk::error::JkError::MalformedFlag { name, .. } = err else {
        panic!("expected MalformedFlag");
    };
    assert_eq!(name, "++config");
}

#[test]
fn duplicate_config_flag_is_malformed() {
    let err = parse_argv(vec![
        "++config=/a".into(),
        "++config=/b".into(),
        "build".into(),
    ])
    .unwrap_err();
    let jk::error::JkError::MalformedFlag { name, reason } = err else {
        panic!("expected MalformedFlag");
    };
    assert_eq!(name, "++config");
    assert!(reason.contains("once"), "reason: {}", reason);
}

#[test]
fn boolean_flag_with_value_is_malformed() {
    let err = parse_argv(vec!["++version=foo".into()]).unwrap_err();
    let jk::error::JkError::MalformedFlag { name, .. } = err else {
        panic!("expected MalformedFlag");
    };
    assert_eq!(name, "++version");
}

#[test]
fn triple_plus_is_unknown_flag() {
    let err = parse_argv(vec!["+++arg".into()]).unwrap_err();
    assert!(matches!(err, jk::error::JkError::UnknownFlag(_)));
}

#[test]
fn bare_double_plus_is_malformed() {
    let err = parse_argv(vec!["++".into()]).unwrap_err();
    let jk::error::JkError::MalformedFlag { name, reason } = err else {
        panic!("expected MalformedFlag, got: {:?}", err);
    };
    assert_eq!(name, "++");
    assert!(reason.contains("missing flag name"), "reason: {}", reason);
}

#[test]
fn double_dash_separator_passes_through_plus_plus_tokens() {
    let r = parse_argv(vec![
        "cmd".into(),
        "--".into(),
        "++version".into(),
        "++bogus".into(),
    ])
    .unwrap();
    assert!(!r.version);
    assert_eq!(r.path, vec!["cmd", "++version", "++bogus"]);
}

#[test]
fn double_dash_separator_first_only() {
    let r = parse_argv(vec![
        "a".into(),
        "--".into(),
        "b".into(),
        "--".into(),
        "c".into(),
    ])
    .unwrap();
    assert_eq!(r.path, vec!["a", "b", "--", "c"]);
}

#[test]
fn double_dash_alone_is_no_op() {
    let r = parse_argv(vec!["a".into(), "--".into()]).unwrap();
    assert_eq!(r.path, vec!["a"]);
}

#[test]
fn flags_before_double_dash_still_parse() {
    let r = parse_argv(vec![
        "++dry-run".into(),
        "cmd".into(),
        "--".into(),
        "++literal".into(),
    ])
    .unwrap();
    assert!(r.dry_run);
    assert_eq!(r.path, vec!["cmd", "++literal"]);
}
