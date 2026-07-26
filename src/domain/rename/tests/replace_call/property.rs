use super::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn pbt_replace_function_calls_output_remains_parseable(
        from in symbol_strategy(),
        to in symbol_strategy(),
        arg in symbol_strategy(),
    ) {
        prop_assume!(from != to);
        prop_assume!(from != arg);
        prop_assume!(to != arg);
        let input = format!("(defun keep () {from})\n({from} {arg})");
        let plan = plan_replace_function_calls(ReplaceFunctionCallsRequest {
            input: &input,
            dialect: Dialect::CommonLisp,
            from: SymbolName::new(from.clone()).unwrap(),
            to: SymbolName::new(to.clone()).unwrap(),
            scope: ReplaceFunctionCallsScope::AllCalls,
        }).unwrap();

        SyntaxTree::parse(&plan.rewritten).unwrap();
        prop_assert!(plan.changed);
        prop_assert_eq!(plan.calls.len(), 1);
        let replaced_call = format!("({to} {arg})");
        let preserved_definition = format!("(defun keep () {from})");
        prop_assert!(plan.rewritten.contains(&replaced_call));
        prop_assert!(plan.rewritten.contains(&preserved_definition));
    }
}
