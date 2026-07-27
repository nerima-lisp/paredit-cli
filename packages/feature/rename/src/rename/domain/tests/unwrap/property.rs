use super::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn pbt_unwrap_function_calls_output_remains_parseable(
        function in symbol_strategy(),
        wrapper in symbol_strategy(),
        arg in symbol_strategy(),
    ) {
        prop_assume!(function != wrapper);
        prop_assume!(function != arg);
        let input = format!("({wrapper} ({function} {arg}))");
        let plan = plan_unwrap_function_calls(UnwrapFunctionCallsRequest {
            input: &input,
            dialect: Dialect::CommonLisp,
            function: SymbolName::new(function.clone()).unwrap(),
            wrapper: SymbolName::new(wrapper.clone()).unwrap(),
            scope: UnwrapFunctionCallsScope::AllCalls,
        }).unwrap();

        SyntaxTree::parse(&plan.rewritten).unwrap();
        prop_assert!(plan.changed);
        prop_assert_eq!(plan.rewritten, format!("({function} {arg})"));
    }

    #[test]
    fn pbt_wrap_then_unwrap_round_trips_simple_calls(
        function in symbol_strategy(),
        wrapper in symbol_strategy(),
        arg in symbol_strategy(),
    ) {
        prop_assume!(function != wrapper);
        prop_assume!(function != arg);
        let input = format!("({function} {arg})");
        let wrapped = plan_wrap_function_calls(WrapFunctionCallsRequest {
            input: &input,
            dialect: Dialect::CommonLisp,
            function: SymbolName::new(function.clone()).unwrap(),
            wrapper: SymbolName::new(wrapper.clone()).unwrap(),
            wrapper_template: None,
            scope: WrapFunctionCallsScope::AllCalls,
        }).unwrap();
        let unwrapped = plan_unwrap_function_calls(UnwrapFunctionCallsRequest {
            input: &wrapped.rewritten,
            dialect: Dialect::CommonLisp,
            function: SymbolName::new(function).unwrap(),
            wrapper: SymbolName::new(wrapper).unwrap(),
            scope: UnwrapFunctionCallsScope::AllCalls,
        }).unwrap();

        SyntaxTree::parse(&unwrapped.rewritten).unwrap();
        prop_assert_eq!(unwrapped.rewritten, input);
    }
}
