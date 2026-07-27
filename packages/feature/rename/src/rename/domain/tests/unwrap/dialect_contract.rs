use super::*;

fn plan(input: &str, dialect: Dialect, function: &str, wrapper: &str) -> UnwrapFunctionCallsPlan {
    plan_unwrap_function_calls(UnwrapFunctionCallsRequest {
        input,
        dialect,
        function: SymbolName::new(function).unwrap(),
        wrapper: SymbolName::new(wrapper).unwrap(),
        scope: UnwrapFunctionCallsScope::AllCalls,
    })
    .unwrap()
}

#[test]
fn supports_known_dialects_with_their_reader_syntax() {
    let cases = [
        (Dialect::CommonLisp, r"(trace (foo #\) #:done #x2a))"),
        (Dialect::EmacsLisp, r"(trace (foo ?\)))"),
        (Dialect::Scheme, "(trace (foo value))"),
        (Dialect::Clojure, r#"(trace (foo #inst "2020-01-01"))"#),
        (Dialect::Janet, "(trace (foo value))"),
        (Dialect::Fennel, "(trace (foo value))"),
    ];

    for (dialect, input) in cases {
        let plan = plan(input, dialect, "foo", "trace");
        assert_eq!(plan.calls.len(), 1, "{}", dialect.label());
        SyntaxTree::parse_with_dialect(&plan.rewritten, dialect).unwrap();
    }
}

#[test]
fn rejects_unknown_before_parsing_malformed_input() {
    let error = plan_unwrap_function_calls(UnwrapFunctionCallsRequest {
        input: ")",
        dialect: Dialect::Unknown,
        function: SymbolName::new("foo").unwrap(),
        wrapper: SymbolName::new("trace").unwrap(),
        scope: UnwrapFunctionCallsScope::AllCalls,
    })
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "unwrap-function-calls requires a known dialect"
    );
}

#[test]
fn common_lisp_matches_case_and_package_qualified_references() {
    let plan = plan(
        "(TRACE (FOO x)) (app:trace (pkg::foo y))",
        Dialect::CommonLisp,
        "foo",
        "trace",
    );

    assert_eq!(plan.calls.len(), 2);
    assert_eq!(plan.rewritten, "(FOO x) (pkg::foo y)");
}

#[test]
fn non_common_lisp_wrapper_and_inner_matching_are_case_sensitive() {
    let plan = plan(
        "(TRACE (foo x)) (trace (FOO y))",
        Dialect::EmacsLisp,
        "foo",
        "trace",
    );

    assert!(plan.calls.is_empty());
    assert_eq!(plan.rewritten, "(TRACE (foo x)) (trace (FOO y))");
}

#[test]
fn local_callable_shadowing_uses_the_dialect_identity_rule() {
    let input = "(flet ((FOO (x) x)) (trace (foo value)))";
    let common_lisp = plan(input, Dialect::CommonLisp, "foo", "trace");
    let emacs_lisp = plan(input, Dialect::EmacsLisp, "foo", "trace");

    assert!(common_lisp.calls.is_empty());
    assert_eq!(emacs_lisp.calls.len(), 1);
    assert_eq!(emacs_lisp.rewritten, "(flet ((FOO (x) x)) (foo value))");
}
