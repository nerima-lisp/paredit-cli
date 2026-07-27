use super::*;

macro_rules! assert_replace_calls {
    (
        input: $input:expr,
        scope: $scope:expr,
        calls: $calls:expr,
        rewritten: $rewritten:expr
    ) => {{
        let plan = plan_replace_function_calls(ReplaceFunctionCallsRequest {
            input: $input,
            dialect: Dialect::CommonLisp,
            from: SymbolName::new("fetch-user").unwrap(),
            to: SymbolName::new("load-user").unwrap(),
            scope: $scope,
        })
        .unwrap();

        assert_eq!(plan.calls.len(), $calls);
        assert_eq!(plan.rewritten, $rewritten);
        SyntaxTree::parse(&plan.rewritten).unwrap();
    }};
    (
        input: $input:expr,
        scope: $scope:expr,
        calls: $calls:expr,
        rewritten_contains: [$($fragment:expr),+ $(,)?]
    ) => {{
        let plan = plan_replace_function_calls(ReplaceFunctionCallsRequest {
            input: $input,
            dialect: Dialect::CommonLisp,
            from: SymbolName::new("fetch-user").unwrap(),
            to: SymbolName::new("load-user").unwrap(),
            scope: $scope,
        })
        .unwrap();

        assert_eq!(plan.calls.len(), $calls);
        $(
            assert!(
                plan.rewritten.contains($fragment),
                "missing fragment `{}` in rewritten output: {}",
                $fragment,
                plan.rewritten
            );
        )+
        SyntaxTree::parse(&plan.rewritten).unwrap();
    }};
}

macro_rules! assert_shadowed_explicit_path {
    ($input:expr) => {{
        let error = plan_replace_function_calls(ReplaceFunctionCallsRequest {
            input: $input,
            dialect: Dialect::CommonLisp,
            from: SymbolName::new("fetch-user").unwrap(),
            to: SymbolName::new("load-user").unwrap(),
            scope: ReplaceFunctionCallsScope::ExplicitPaths(vec!["0.3.2".parse::<Path>().unwrap()]),
        })
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("call-path 0.3.2 is shadowed by a local callable named fetch-user")
        );
    }};
}

mod basic_forms;
mod dialect_contract;
mod local_callables;
mod macro_forms;
mod property;
