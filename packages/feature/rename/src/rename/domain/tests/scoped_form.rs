use super::*;

#[test]
fn plans_rename_in_form_only_inside_selected_form() {
    let input = "(list value (list value other))\n(list value)";
    let plan = plan_rename_in_form(RenameInFormRequest {
        input,
        dialect: Dialect::CommonLisp,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("value").unwrap(),
        to: SymbolName::new("product").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.path, Some(Path::from_indexes(vec![0])));
    assert_eq!(plan.occurrences.len(), 2);
    assert!(plan.changed);
    assert_eq!(
        plan.rewritten,
        "(list product (list product other))\n(list value)"
    );
    SyntaxTree::parse_with_dialect(&plan.rewritten, Dialect::CommonLisp).unwrap();
}

#[test]
fn rename_in_form_preserves_dialect_reader_collisions() {
    let cases = [(
        Dialect::Janet,
        "(list value)\n# ignored ))",
        "(list product)\n# ignored ))",
    )];

    for (dialect, input, expected) in cases {
        let plan = plan_rename_in_form(RenameInFormRequest {
            input,
            dialect,
            target: RenameTarget::Path(Path::from_indexes(vec![0])),
            from: SymbolName::new("value").unwrap(),
            to: SymbolName::new("product").unwrap(),
        })
        .expect("plan");

        assert_eq!(plan.rewritten, expected);
        SyntaxTree::parse_with_dialect(&plan.rewritten, dialect)
            .expect("rewritten output remains parseable");
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn pbt_rename_in_form_output_remains_parseable_and_scoped(
        from in symbol_strategy(),
        to in symbol_strategy(),
        other in symbol_strategy(),
    ) {
        prop_assume!(from != to);
        prop_assume!(from != other);
        prop_assume!(to != other);
        let input = format!("(list {from} (list {from} {other}))\n(list {from})");
        let plan = plan_rename_in_form(RenameInFormRequest {
            input: &input,
            dialect: Dialect::CommonLisp,
            target: RenameTarget::Path(Path::from_indexes(vec![0])),
            from: SymbolName::new(from.clone()).unwrap(),
            to: SymbolName::new(to.clone()).unwrap(),
        }).unwrap();

        SyntaxTree::parse(&plan.rewritten).unwrap();
        prop_assert!(plan.changed);
        prop_assert_eq!(plan.occurrences.len(), 2);
        prop_assert_eq!(
            plan.rewritten,
            format!("(list {to} (list {to} {other}))\n(list {from})")
        );
    }
}
