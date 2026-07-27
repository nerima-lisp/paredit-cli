use super::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn pbt_wrap_function_calls_output_remains_parseable(
        function in symbol_strategy(),
        wrapper in symbol_strategy(),
        arg in symbol_strategy(),
    ) {
        prop_assume!(function != wrapper);
        prop_assume!(function != arg);
        let input = format!("({function} {arg})");
        let plan = plan_wrap_function_calls(WrapFunctionCallsRequest {
            input: &input,
            dialect: Dialect::CommonLisp,
            function: SymbolName::new(function.clone()).unwrap(),
            wrapper: SymbolName::new(wrapper.clone()).unwrap(),
            wrapper_template: None,
            scope: WrapFunctionCallsScope::AllCalls,
        }).unwrap();

        SyntaxTree::parse(&plan.rewritten).unwrap();
        prop_assert!(plan.changed);
        prop_assert_eq!(plan.rewritten, format!("({wrapper} ({function} {arg}))"));
    }
}
