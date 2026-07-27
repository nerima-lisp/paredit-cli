use super::*;

macro_rules! assert_wrap_calls {
    (
        input: $input:expr,
        function: $function:expr,
        wrapper: $wrapper:expr,
        wrapper_template: $wrapper_template:expr,
        scope: $scope:expr,
        calls: $calls:expr,
        rewritten: $rewritten:expr
    ) => {{
        let plan = plan_wrap_function_calls(WrapFunctionCallsRequest {
            input: $input,
            dialect: Dialect::CommonLisp,
            function: SymbolName::new($function).unwrap(),
            wrapper: SymbolName::new($wrapper).unwrap(),
            wrapper_template: $wrapper_template,
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
        let plan = plan_wrap_function_calls(WrapFunctionCallsRequest {
            input: $input,
            dialect: Dialect::CommonLisp,
            function: SymbolName::new("fetch-user").unwrap(),
            wrapper: SymbolName::new("with-cache").unwrap(),
            wrapper_template: None,
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

macro_rules! assert_shadowed_wrap_explicit_path {
    ($input:expr) => {{
        let error = plan_wrap_function_calls(WrapFunctionCallsRequest {
            input: $input,
            dialect: Dialect::CommonLisp,
            function: SymbolName::new("fetch-user").unwrap(),
            wrapper: SymbolName::new("with-cache").unwrap(),
            wrapper_template: None,
            scope: WrapFunctionCallsScope::ExplicitPaths(vec!["0.3.2".parse::<Path>().unwrap()]),
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
