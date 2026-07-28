use super::*;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SymbolName;
use paredit_core_syntax::sexpr::SyntaxTree;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn pbt_rename_labels_output_remains_parseable_and_updates_recursive_calls(
        from in symbol_strategy(),
        to in symbol_strategy(),
    ) {
        prop_assume!(from != to);
        let input = format!("(labels (({from} (x) ({from} x))) ({from} 1) {from})");
        let plan = plan_rename_local_function(RenameLocalFunctionRequest {
            input: &input,
            dialect: Dialect::CommonLisp,
            from: SymbolName::new(from.clone()).unwrap(),
            to: SymbolName::new(to.clone()).unwrap(),
        }).unwrap();

        SyntaxTree::parse(&plan.rewritten).unwrap();
        prop_assert!(plan.changed);
        let rewritten_definition = format!("(labels (({to} (x)");
        let rewritten_recursive_call = format!("({to} x)");
        let rewritten_body_call = format!("({to} 1)");

        prop_assert!(plan.rewritten.contains(&rewritten_definition));
        prop_assert!(plan.rewritten.contains(&rewritten_recursive_call));
        prop_assert!(plan.rewritten.contains(&rewritten_body_call));
    }
}
