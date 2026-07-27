use super::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn pbt_rename_symbol_macro_output_remains_parseable_and_skips_call_heads(
        from in symbol_strategy(),
        to in symbol_strategy(),
    ) {
        prop_assume!(from != to);
        let input = format!(
            "(define-symbol-macro {from} current-user) (list {from} ({from} 1) (setf {from} 2))"
        );
        let plan = plan_rename_symbol_macro(RenameSymbolMacroRequest {
            input: &input,
            dialect: Dialect::CommonLisp,
            from: SymbolName::new(from.clone()).unwrap(),
            to: SymbolName::new(to.clone()).unwrap(),
        }).unwrap();

        SyntaxTree::parse(&plan.rewritten).unwrap();
        prop_assert!(plan.changed);
        let expected_definition = format!("(define-symbol-macro {to} current-user)");
        let expected_reference = format!("(list {to} ({from} 1) (setf {to} 2))");
        prop_assert!(plan.rewritten.contains(&expected_definition));
        prop_assert!(plan.rewritten.contains(&expected_reference));
    }
}
