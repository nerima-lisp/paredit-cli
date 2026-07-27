use super::*;

#[test]
fn replaces_call_heads_without_touching_definitions_or_value_references() {
    assert_replace_calls!(
        input: "(defun fetch-user (id) (list fetch-user id))\n(defun render () (fetch-user id))",
        scope: ReplaceFunctionCallsScope::AllCalls,
        calls: 1,
        rewritten:
            "(defun fetch-user (id) (list fetch-user id))\n(defun render () (load-user id))"
    );
}

#[test]
fn replaces_only_explicit_call_paths() {
    let plan = plan_replace_function_calls(ReplaceFunctionCallsRequest {
        input: "(defun render () (fetch-user id) (fetch-user other))",
        dialect: Dialect::CommonLisp,
        from: SymbolName::new("fetch-user").unwrap(),
        to: SymbolName::new("load-user").unwrap(),
        scope: ReplaceFunctionCallsScope::ExplicitPaths(vec!["0.4".parse::<Path>().unwrap()]),
    })
    .unwrap();

    assert_eq!(plan.calls.len(), 1);
    assert_eq!(plan.calls[0].path, "0.4");
    assert_eq!(
        plan.rewritten,
        "(defun render () (fetch-user id) (load-user other))"
    );
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn explicit_path_must_select_matching_call() {
    let error = plan_replace_function_calls(ReplaceFunctionCallsRequest {
        input: "(defun render () (fetch-user id))",
        dialect: Dialect::CommonLisp,
        from: SymbolName::new("fetch-user").unwrap(),
        to: SymbolName::new("load-user").unwrap(),
        scope: ReplaceFunctionCallsScope::ExplicitPaths(vec!["0".parse::<Path>().unwrap()]),
    })
    .unwrap_err();

    assert!(error.to_string().contains("call-path 0 is not a call"));
}
