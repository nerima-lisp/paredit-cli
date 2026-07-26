use super::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn pbt_rename_function_output_remains_parseable_and_preserves_value_refs(
        from in symbol_strategy(),
        to in symbol_strategy(),
    ) {
        prop_assume!(from != to);
        let input = format!("(defun {from} (x) (list {from} x))\n(defun caller () ({from} 1))");
        let plan = plan_rename_function(RenameFunctionRequest {
            input: &input,
            dialect: Dialect::CommonLisp,
            from: SymbolName::new(from.clone()).unwrap(),
            to: SymbolName::new(to.clone()).unwrap(),
        }).unwrap();

        SyntaxTree::parse(&plan.rewritten).unwrap();
        prop_assert!(plan.changed);
        let rewritten_definition = format!("(defun {to} (x)");
        let rewritten_call = format!("({to} 1)");
        let preserved_value_reference = format!("(list {from} x)");

        prop_assert!(plan.rewritten.contains(&rewritten_definition));
        prop_assert!(plan.rewritten.contains(&rewritten_call));
        prop_assert!(plan.rewritten.contains(&preserved_value_reference));
    }
}
