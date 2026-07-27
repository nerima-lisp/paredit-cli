use super::*;

fn plan(input: &str, dialect: Dialect, from: &str, to: &str) -> ReplaceFunctionCallsPlan {
    plan_replace_function_calls(ReplaceFunctionCallsRequest {
        input,
        dialect,
        from: SymbolName::new(from).unwrap(),
        to: SymbolName::new(to).unwrap(),
        scope: ReplaceFunctionCallsScope::AllCalls,
    })
    .unwrap()
}

#[test]
fn supports_known_dialects_with_their_reader_syntax() {
    let cases = [
        (Dialect::CommonLisp, r"(foo #\) #:done #x2a)"),
        (Dialect::EmacsLisp, r"(foo ?\))"),
        (Dialect::Scheme, "(foo value)"),
        (Dialect::Clojure, r#"(foo #inst "2020-01-01")"#),
        (Dialect::Janet, "(foo value)"),
        (Dialect::Fennel, "(foo value)"),
    ];

    for (dialect, input) in cases {
        let plan = plan(input, dialect, "foo", "bar");
        assert_eq!(plan.calls.len(), 1, "{}", dialect.label());
        SyntaxTree::parse_with_dialect(&plan.rewritten, dialect).unwrap();
    }
}

#[test]
fn rejects_unknown_before_parsing_malformed_input() {
    let error = plan_replace_function_calls(ReplaceFunctionCallsRequest {
        input: ")",
        dialect: Dialect::Unknown,
        from: SymbolName::new("foo").unwrap(),
        to: SymbolName::new("bar").unwrap(),
        scope: ReplaceFunctionCallsScope::AllCalls,
    })
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "replace-function-calls requires a known dialect"
    );
}

#[test]
fn common_lisp_matches_case_and_package_qualified_references() {
    let plan = plan("(FOO x) (app::foo y)", Dialect::CommonLisp, "foo", "bar");

    assert_eq!(plan.calls.len(), 2);
    assert_eq!(plan.rewritten, "(bar x) (bar y)");
}

#[test]
fn non_common_lisp_matching_is_case_sensitive() {
    let plan = plan("(FOO value)", Dialect::EmacsLisp, "foo", "bar");

    assert!(plan.calls.is_empty());
    assert_eq!(plan.rewritten, "(FOO value)");
}

#[test]
fn local_callable_shadowing_uses_the_dialect_identity_rule() {
    let input = "(flet ((FOO (x) x)) (foo value))";
    let common_lisp = plan(input, Dialect::CommonLisp, "foo", "bar");
    let emacs_lisp = plan(input, Dialect::EmacsLisp, "foo", "bar");

    assert!(common_lisp.calls.is_empty());
    assert_eq!(emacs_lisp.calls.len(), 1);
    assert_eq!(emacs_lisp.rewritten, "(flet ((FOO (x) x)) (bar value))");
}
