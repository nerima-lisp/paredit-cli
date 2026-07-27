use super::*;

#[test]
fn wraps_outermost_calls_and_skips_nested_and_existing() {
    let plan = plan_wrap_function_calls(WrapFunctionCallsRequest {
        input: "(foo (foo x))\n(trace (foo y))",
        dialect: Dialect::CommonLisp,
        function: SymbolName::new("foo").unwrap(),
        wrapper: SymbolName::new("trace").unwrap(),
        wrapper_template: None,
        scope: WrapFunctionCallsScope::AllCalls,
    })
    .unwrap();

    assert_eq!(plan.calls.len(), 1);
    assert_eq!(plan.skipped_nested.len(), 1);
    assert_eq!(plan.skipped_already_wrapped.len(), 1);
    assert_eq!(plan.rewritten, "(trace (foo (foo x)))\n(trace (foo y))");
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn wraps_calls_with_macro_friendly_template() {
    assert_wrap_calls!(
        input: "(foo x)",
        function: "foo",
        wrapper: "with-tracing",
        wrapper_template: Some("(with-tracing :label 'foo _)".to_owned()),
        scope: WrapFunctionCallsScope::AllCalls,
        calls: 1,
        rewritten: "(with-tracing :label 'foo (foo x))"
    );
}

#[test]
fn rejects_wrapper_templates_without_exactly_one_placeholder() {
    let error = plan_wrap_function_calls(WrapFunctionCallsRequest {
        input: "(foo x)",
        dialect: Dialect::CommonLisp,
        function: SymbolName::new("foo").unwrap(),
        wrapper: SymbolName::new("with-tracing").unwrap(),
        wrapper_template: Some("(with-tracing :label 'foo)".to_owned()),
        scope: WrapFunctionCallsScope::AllCalls,
    })
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("wrapper template must contain exactly one _ placeholder atom")
    );
}
