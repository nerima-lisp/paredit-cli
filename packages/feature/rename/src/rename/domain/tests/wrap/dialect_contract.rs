use super::*;

fn plan(
    input: &str,
    dialect: Dialect,
    function: &str,
    wrapper: &str,
    wrapper_template: Option<String>,
) -> WrapFunctionCallsPlan {
    plan_wrap_function_calls(WrapFunctionCallsRequest {
        input,
        dialect,
        function: SymbolName::new(function).unwrap(),
        wrapper: SymbolName::new(wrapper).unwrap(),
        wrapper_template,
        scope: WrapFunctionCallsScope::AllCalls,
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
        let plan = plan(input, dialect, "foo", "trace", None);
        assert_eq!(plan.calls.len(), 1, "{}", dialect.label());
        SyntaxTree::parse_with_dialect(&plan.rewritten, dialect).unwrap();
    }
}

#[test]
fn rejects_unknown_before_parsing_input_or_template() {
    let error = plan_wrap_function_calls(WrapFunctionCallsRequest {
        input: ")",
        dialect: Dialect::Unknown,
        function: SymbolName::new("foo").unwrap(),
        wrapper: SymbolName::new("trace").unwrap(),
        wrapper_template: Some("(".to_owned()),
        scope: WrapFunctionCallsScope::AllCalls,
    })
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "wrap-function-calls requires a known dialect"
    );
}

#[test]
fn common_lisp_matches_case_and_package_qualified_references() {
    let plan = plan(
        "(FOO x) (app::foo y)",
        Dialect::CommonLisp,
        "foo",
        "trace",
        Some("(TRACE _)".to_owned()),
    );

    assert_eq!(plan.calls.len(), 2);
    assert_eq!(plan.rewritten, "(TRACE (FOO x)) (TRACE (app::foo y))");
}

#[test]
fn non_common_lisp_matching_and_template_heads_are_case_sensitive() {
    let plan = plan("(FOO value)", Dialect::EmacsLisp, "foo", "trace", None);
    assert!(plan.calls.is_empty());

    let error = plan_wrap_function_calls(WrapFunctionCallsRequest {
        input: "(foo value)",
        dialect: Dialect::EmacsLisp,
        function: SymbolName::new("foo").unwrap(),
        wrapper: SymbolName::new("trace").unwrap(),
        wrapper_template: Some("(TRACE _)".to_owned()),
        scope: WrapFunctionCallsScope::AllCalls,
    })
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("wrapper template head must match --wrapper (trace)")
    );
}

#[test]
fn already_wrapped_detection_uses_exact_identity_outside_common_lisp() {
    let plan = plan(
        "(TRACE (foo value))",
        Dialect::EmacsLisp,
        "foo",
        "trace",
        None,
    );

    assert_eq!(plan.calls.len(), 1);
    assert!(plan.skipped_already_wrapped.is_empty());
    assert_eq!(plan.rewritten, "(TRACE (trace (foo value)))");
}

#[test]
fn local_callable_shadowing_uses_the_dialect_identity_rule() {
    let input = "(flet ((FOO (x) x)) (foo value))";
    let common_lisp = plan(input, Dialect::CommonLisp, "foo", "trace", None);
    let emacs_lisp = plan(input, Dialect::EmacsLisp, "foo", "trace", None);

    assert!(common_lisp.calls.is_empty());
    assert_eq!(emacs_lisp.calls.len(), 1);
    assert_eq!(
        emacs_lisp.rewritten,
        "(flet ((FOO (x) x)) (trace (foo value)))"
    );
}
