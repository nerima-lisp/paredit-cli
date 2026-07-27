use super::*;

#[test]
fn unwraps_outermost_unary_wrappers_and_skips_unsafe_sites() {
    let plan = plan_unwrap_function_calls(UnwrapFunctionCallsRequest {
        input: "(trace (foo (trace (foo x))))\n(trace (foo y) :label \"y\")",
        dialect: Dialect::CommonLisp,
        function: SymbolName::new("foo").unwrap(),
        wrapper: SymbolName::new("trace").unwrap(),
        scope: UnwrapFunctionCallsScope::AllCalls,
    })
    .unwrap();

    assert_eq!(plan.calls.len(), 1);
    assert_eq!(plan.skipped_nested.len(), 1);
    assert_eq!(plan.skipped_non_unary_wrapper.len(), 1);
    assert_eq!(
        plan.rewritten,
        "(foo (trace (foo x)))\n(trace (foo y) :label \"y\")"
    );
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn explicit_path_rejects_non_wrapper_targets() {
    let error = plan_unwrap_function_calls(UnwrapFunctionCallsRequest {
        input: "(defun render () (foo x))",
        dialect: Dialect::CommonLisp,
        function: SymbolName::new("foo").unwrap(),
        wrapper: SymbolName::new("trace").unwrap(),
        scope: UnwrapFunctionCallsScope::ExplicitPaths(vec![Path::from_indexes(vec![0, 2])]),
    })
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("call-path 0.2 is not a unary trace wrapper around foo")
    );
}
