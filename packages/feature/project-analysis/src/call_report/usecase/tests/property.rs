use super::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn pbt_reports_generated_call_heads_and_argument_counts(
        function in symbol_strategy(),
        callee in symbol_strategy(),
        argument_count in 0usize..6,
    ) {
        prop_assume!(function != callee);
        let args = (0..argument_count)
            .map(|index| format!("x{index}"))
            .collect::<Vec<_>>()
            .join(" ");
        let input = format!(
            "(defun {function} () ({callee}{prefix}{args}))",
            prefix = if args.is_empty() { "" } else { " " }
        );
        let tree = SyntaxTree::parse(&input).unwrap();
        let symbol = SymbolName::new(callee.clone()).unwrap();
        let calls = build_call_report(&tree, Dialect::CommonLisp, Some(&symbol), false).unwrap();

        prop_assert_eq!(calls.len(), 1);
        prop_assert_eq!(calls[0].head.as_str(), callee.as_str());
        prop_assert_eq!(calls[0].argument_count, argument_count);
        prop_assert_eq!(calls[0].enclosing_definition.as_deref(), Some(function.as_str()));
    }
}
