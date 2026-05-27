use jk::render::{aggregate_shape, analyze, fold, substitute, validate_args};
use jk::shell::Shell;

#[test]
fn no_placeholders_no_args() {
    let r = substitute("apt update", &[], Shell::Bash).unwrap();
    assert_eq!(r, "apt update");
}

#[test]
fn substitute_positional_quoted() {
    let r = substitute("magick #{1}", &["in 1.png".into()], Shell::Bash).unwrap();
    assert_eq!(r, "magick 'in 1.png'");
}

#[test]
fn substitute_at_quotes_each() {
    let r = substitute("apt upgrade #{@}", &["-y".into(), "x y".into()], Shell::Bash).unwrap();
    assert_eq!(r, "apt upgrade -y 'x y'");
}

#[test]
fn at_means_remaining_not_all() {
    // Spec: #{@} = args[max_n..]. Here max_n=1, so #{@} skips args[0].
    let r = substitute(
        "first=#{1} all=#{@}",
        &["a".into(), "b".into(), "c".into()],
        Shell::Bash,
    )
    .unwrap();
    assert_eq!(r, "first=a all=b c");
}

#[test]
fn at_with_no_remaining_args_renders_empty() {
    // #{1} #{@} with exactly 1 arg: #{@} = empty slice → empty string.
    let r = substitute("a=#{1} rest=#{@}", &["only".into()], Shell::Bash).unwrap();
    assert_eq!(r, "a=only rest=");
}

#[test]
fn at_alone_with_no_args_renders_empty() {
    let r = substitute("only-at #{@}", &[], Shell::Bash).unwrap();
    assert_eq!(r, "only-at ");
}

#[test]
fn substitute_pwsh_quoting() {
    let r = substitute("Get-Item #{1}", &["it's".into()], Shell::Pwsh).unwrap();
    assert_eq!(r, "Get-Item 'it''s'");
}

#[test]
fn missing_placeholder_arg_errors() {
    let shape = aggregate_shape(&["magick #{1} -quality #{2} #{3}"]).unwrap();
    let err = validate_args(&["in.png".into()], shape).unwrap_err();
    let jk::error::JkError::MissingArg(s) = err else { panic!() };
    assert!(s.contains("#{2}"), "expected #{{2}} in: {}", s);
}

#[test]
fn extra_args_without_at_errors() {
    let shape = aggregate_shape(&["apt upgrade"]).unwrap();
    let err = validate_args(&["-y".into()], shape).unwrap_err();
    assert!(matches!(err, jk::error::JkError::ExtraArgs(1)));
}

#[test]
fn raw_form_skips_quoting() {
    let r = substitute("--vf=#{1!}", &["scale=W:H, format=yuv".into()], Shell::Bash).unwrap();
    assert_eq!(r, "--vf=scale=W:H, format=yuv");
}

#[test]
fn raw_at_skips_quoting() {
    let r = substitute("cmd #{@!}", &["a b".into(), "c".into()], Shell::Bash).unwrap();
    assert_eq!(r, "cmd a b c");
}

#[test]
fn raw_and_quoted_can_coexist() {
    // #{1} quoted, #{1!} raw — both reference the same arg, in different ways.
    let r = substitute("q=#{1} r=#{1!}", &["x y".into()], Shell::Bash).unwrap();
    assert_eq!(r, "q='x y' r=x y");
}

#[test]
fn raw_at_means_remaining() {
    let r = substitute(
        "first=#{1} rest=#{@!}",
        &["a".into(), "b c".into(), "d".into()],
        Shell::Bash,
    )
    .unwrap();
    assert_eq!(r, "first=a rest=b c d");
}

#[test]
fn analyze_accepts_valid_templates() {
    assert!(analyze("no placeholders").is_ok());
    assert!(analyze("just #{1}").is_ok());
    assert!(analyze("#{@}").is_ok());
    assert!(analyze("#{1!} #{@!}").is_ok());
    assert!(analyze("#{3} #{2} #{1}").is_ok()); // out-of-order is fine if contiguous
    assert!(analyze("#{1} #{1}").is_ok()); // duplicate is fine
    assert!(analyze("#{@} pipe #{@}").is_ok()); // multiple #{@} fine
}

#[test]
fn analyze_rejects_zero_index() {
    let err = analyze("echo #{0}").unwrap_err();
    let jk::error::JkError::ConfigSchema(msg) = err else { panic!() };
    assert!(msg.contains("#{0}"), "msg: {}", msg);
}

#[test]
fn analyze_rejects_non_numeric() {
    for tmpl in [
        "echo #{abc}",
        "echo #{1.5}",
        "echo #{-1}",
        "echo #{+1}",
        "echo #{01}",
        "echo #{!1}",
    ] {
        let err = analyze(tmpl).unwrap_err();
        assert!(matches!(err, jk::error::JkError::ConfigSchema(_)), "tmpl: {}", tmpl);
    }
}

#[test]
fn analyze_does_not_check_contiguity_per_item() {
    let s = analyze("echo #{1} #{3}").unwrap();
    assert_eq!(s.max_n, 3);
}

#[test]
fn analyze_does_not_reject_starting_above_one() {
    let s = analyze("echo #{2}").unwrap();
    assert_eq!(s.max_n, 2);
}

#[test]
fn analyze_rejects_unclosed_placeholder() {
    let err = analyze("echo #{1").unwrap_err();
    assert!(matches!(err, jk::error::JkError::ConfigSchema(_)));
}

#[test]
fn analyze_returns_correct_shape() {
    let s = analyze("first=#{1} second=#{2!} all=#{@}").unwrap();
    assert_eq!(s.max_n, 2);
    assert!(s.has_at);

    let s = analyze("only #{@!}").unwrap();
    assert_eq!(s.max_n, 0);
    assert!(s.has_at);

    let s = analyze("only #{1}").unwrap();
    assert_eq!(s.max_n, 1);
    assert!(!s.has_at);
}

#[test]
fn analyze_accepts_literal_dollar_brace() {
    assert!(analyze("echo ${HOME}").is_ok());
    assert!(analyze("echo ${VAR:-default}").is_ok());
    assert!(analyze("echo ${var#prefix}").is_ok());
    assert!(analyze("echo ${#var}").is_ok());
}

#[test]
fn analyze_accepts_literal_awk_jq_blocks() {
    assert!(analyze("awk '{print $1}' input.txt").is_ok());
    assert!(analyze("jq '{name: .name, age: .age}'").is_ok());
}

#[test]
fn analyze_accepts_literal_double_brace() {
    assert!(analyze("helm template my-chart --set value={{X}}").is_ok());
    assert!(analyze("echo {{1}}").is_ok());
    assert!(analyze("gomplate --in '{{.Env.HOME}}'").is_ok());
}

#[test]
fn analyze_accepts_literal_brace_expansion() {
    assert!(analyze("echo {a,b,c}.txt").is_ok());
    assert!(analyze("echo {1..10}").is_ok());
}

#[test]
fn analyze_accepts_literal_hash() {
    assert!(analyze("echo hello # this is a comment").is_ok());
    assert!(analyze("curl -# https://example.com").is_ok());
    assert!(analyze("echo a#b#c").is_ok());
}

#[test]
fn substitute_preserves_literal_braces_and_hash() {
    let r = substitute("echo ${HOME} #{1}", &["x".into()], Shell::Bash).unwrap();
    assert_eq!(r, "echo ${HOME} x");

    let r = substitute("awk '{print $1}' #{1}", &["file".into()], Shell::Bash).unwrap();
    assert_eq!(r, "awk '{print $1}' file");

    let r = substitute("echo {{X}} #{@}", &["a".into()], Shell::Bash).unwrap();
    assert_eq!(r, "echo {{X}} a");

    let r = substitute("echo #foo bar#", &[], Shell::Bash).unwrap();
    assert_eq!(r, "echo #foo bar#");
}

#[test]
fn substitute_double_hash_then_placeholder() {
    // First `#` is literal (peek is `#`); the second `#` triggers the placeholder.
    let r = substitute("echo ##{1}", &["x".into()], Shell::Bash).unwrap();
    assert_eq!(r, "echo #x");
}

#[test]
fn fold_single_line_unchanged() {
    assert_eq!(fold("apt update"), "apt update");
}

#[test]
fn fold_multiline_args() {
    let s = "
docker run
  --rm
  -v /a:/b
  alpine sh
";
    assert_eq!(fold(s), "docker run --rm -v /a:/b alpine sh");
}

#[test]
fn fold_drops_empty_lines() {
    assert_eq!(fold("cmd1\n\n\ncmd2"), "cmd1 cmd2");
}

#[test]
fn fold_trims_per_line() {
    assert_eq!(fold("  hello  \n   world  "), "hello world");
}

#[test]
fn aggregate_shape_single_template() {
    let s = aggregate_shape(&["magick #{1} #{2}"]).unwrap();
    assert_eq!(s.max_n, 2);
    assert!(!s.has_at);
}

#[test]
fn aggregate_shape_max_n_takes_max() {
    let s = aggregate_shape(&["jk clean", "jk build #{1}", "jk package #{1} #{2} #{3}"]).unwrap();
    assert_eq!(s.max_n, 3);
    assert!(!s.has_at);
}

#[test]
fn aggregate_shape_has_at_is_any() {
    let s = aggregate_shape(&["echo #{1}", "echo #{1} #{2}"]).unwrap();
    assert!(!s.has_at);

    let s = aggregate_shape(&["apt update", "apt upgrade #{@}"]).unwrap();
    assert!(s.has_at);
    assert_eq!(s.max_n, 0);

    let s = aggregate_shape(&["a", "cmd #{@!}"]).unwrap();
    assert!(s.has_at);
}

#[test]
fn aggregate_shape_release_example_from_spec() {
    // Spec [release] example: cmd = ["jk clean", "jk build", "jk package #{1}"]
    let s = aggregate_shape(&["jk clean", "jk build", "jk package #{1}"]).unwrap();
    assert_eq!(s.max_n, 1);
    assert!(!s.has_at);
}

#[test]
fn aggregate_shape_propagates_analyze_errors() {
    let err = aggregate_shape(&["echo #{1}", "echo #{0}"]).unwrap_err();
    assert!(matches!(err, jk::error::JkError::ConfigSchema(_)));
}

#[test]
fn aggregate_shape_rejects_gap_within_single_item() {
    let err = aggregate_shape(&["echo #{1} #{3}"]).unwrap_err();
    let jk::error::JkError::ConfigSchema(msg) = err else { panic!() };
    assert!(msg.contains("non-contiguous") || msg.contains("missing"), "msg: {}", msg);
}

#[test]
fn aggregate_shape_rejects_starting_above_one() {
    let err = aggregate_shape(&["echo #{2}"]).unwrap_err();
    assert!(matches!(err, jk::error::JkError::ConfigSchema(_)));
}

#[test]
fn aggregate_shape_accepts_split_indices_across_items() {
    let s = aggregate_shape(&["echo #{1}", "echo #{2}"]).unwrap();
    assert_eq!(s.max_n, 2);
    assert!(!s.has_at);
}

#[test]
fn aggregate_shape_rejects_split_with_gap() {
    let err = aggregate_shape(&["echo #{1}", "echo #{3}"]).unwrap_err();
    let jk::error::JkError::ConfigSchema(msg) = err else { panic!() };
    assert!(msg.contains("missing") || msg.contains("non-contiguous"), "msg: {}", msg);
}

#[test]
fn validate_args_accepts_exact_max_n() {
    let shape = aggregate_shape(&["echo #{1} #{2}"]).unwrap();
    assert!(validate_args(&["a".into(), "b".into()], shape).is_ok());
}

#[test]
fn validate_args_accepts_more_when_has_at() {
    let shape = aggregate_shape(&["apt update", "apt upgrade #{@}"]).unwrap();
    assert!(validate_args(&[], shape).is_ok());
    assert!(validate_args(&["-y".into(), "pkg".into()], shape).is_ok());
}

#[test]
fn validate_args_rejects_extra_when_no_at() {
    let shape = aggregate_shape(&["jk clean", "jk build", "jk package #{1}"]).unwrap();
    let err = validate_args(&["v1.5".into(), "extra".into()], shape).unwrap_err();
    assert!(matches!(err, jk::error::JkError::ExtraArgs(1)));
}

#[test]
fn validate_args_rejects_missing() {
    let shape = aggregate_shape(&["echo #{1} #{2} #{3}"]).unwrap();
    let err = validate_args(&["a".into()], shape).unwrap_err();
    let jk::error::JkError::MissingArg(s) = err else { panic!() };
    assert!(s.contains("#{2}"), "expected #{{2}}, got: {}", s);
}

#[test]
fn fold_then_substitute_preserves_arg_newlines() {
    let template = "echo #{1}";
    let folded = fold(template);
    let result = substitute(&folded, &["foo\nbar".into()], Shell::Bash).unwrap();
    assert!(result.contains('\n'), "expected newline preserved in: {:?}", result);
    assert_eq!(result, "echo 'foo\nbar'");
}

#[test]
fn fold_only_acts_on_template_not_value() {
    let template = "echo\n  #{1}";
    let folded = fold(template);
    assert_eq!(folded, "echo #{1}");
    let r = substitute(&folded, &["a\nb".into()], Shell::Bash).unwrap();
    assert!(r.contains('\n'));
}

#[test]
fn per_item_at_slice_uses_item_own_max_n() {
    // #{@} slices from this item's own max_n, not the leaf aggregate's.
    let r0 = substitute("echo #{@}", &["a".into(), "b".into()], Shell::Bash).unwrap();
    assert_eq!(r0, "echo a b");
    let r1 = substitute("echo #{1}", &["a".into(), "b".into()], Shell::Bash).unwrap();
    assert_eq!(r1, "echo a");
}
