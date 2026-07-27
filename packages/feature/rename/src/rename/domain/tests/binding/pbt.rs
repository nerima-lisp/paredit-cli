use super::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn pbt_rename_binding_output_remains_parseable_and_scope_aware(
        from in symbol_strategy(),
        to in symbol_strategy(),
        other in symbol_strategy(),
    ) {
        prop_assume!(from != to);
        prop_assume!(from != other);
        prop_assume!(to != other);
        let input = format!("(let (({from} 1) ({other} {from})) (list {from} {other}))");
        let plan = plan_rename_binding(RenameBindingRequest {
            input: &input,
            dialect: Dialect::CommonLisp,
            target: RenameTarget::Path(Path::from_indexes(vec![0])),
            from: SymbolName::new(from.clone()).unwrap(),
            to: SymbolName::new(to.clone()).unwrap(),
        }).unwrap();

        SyntaxTree::parse(&plan.rewritten).unwrap();
        prop_assert!(plan.changed);
        prop_assert_eq!(plan.references.len(), 1);
        prop_assert_eq!(
            plan.rewritten,
            format!("(let (({to} 1) ({other} {from})) (list {to} {other}))")
        );
    }

    #[test]
    fn pbt_rename_lambda_parameter_output_remains_parseable_and_scope_aware(
        from in symbol_strategy(),
        to in symbol_strategy(),
        other in symbol_strategy(),
    ) {
        prop_assume!(from != to);
        prop_assume!(from != other);
        prop_assume!(to != other);
        let input = format!("(lambda ({from} {other}) (list {from} (lambda ({from}) {from}) {other}))");
        let plan = plan_rename_binding(RenameBindingRequest {
            input: &input,
            dialect: Dialect::CommonLisp,
            target: RenameTarget::Path(Path::from_indexes(vec![0])),
            from: SymbolName::new(from.clone()).unwrap(),
            to: SymbolName::new(to.clone()).unwrap(),
        }).unwrap();

        SyntaxTree::parse(&plan.rewritten).unwrap();
        prop_assert!(plan.changed);
        prop_assert_eq!(plan.references.len(), 1);
        prop_assert_eq!(plan.shadowed_scope_count, 1);
        prop_assert_eq!(
            plan.rewritten,
            format!("(lambda ({to} {other}) (list {to} (lambda ({from}) {from}) {other}))")
        );
    }

    #[test]
    fn pbt_rename_destructured_fn_parameter_output_remains_parseable_and_scope_aware(
        from in symbol_strategy(),
        to in symbol_strategy(),
        other in symbol_strategy(),
    ) {
        prop_assume!(from != to);
        prop_assume!(from != other);
        prop_assume!(to != other);
        let input = format!("(fn [[{from} {other}]] (list {from} (fn [{from}] {from}) {other}))");
        let plan = plan_rename_binding(RenameBindingRequest {
            input: &input,
            dialect: Dialect::Clojure,
            target: RenameTarget::Path(Path::from_indexes(vec![0])),
            from: SymbolName::new(from.clone()).unwrap(),
            to: SymbolName::new(to.clone()).unwrap(),
        }).unwrap();

        SyntaxTree::parse(&plan.rewritten).unwrap();
        prop_assert!(plan.changed);
        prop_assert_eq!(plan.references.len(), 1);
        prop_assert_eq!(plan.shadowed_scope_count, 1);
        prop_assert_eq!(
            plan.rewritten,
            format!("(fn [[{to} {other}]] (list {to} (fn [{from}] {from}) {other}))")
        );
    }

    #[test]
    fn pbt_rename_clojure_keys_destructured_fn_parameter_preserves_lookup_key(
        from in symbol_strategy(),
        to in symbol_strategy(),
        other in symbol_strategy(),
    ) {
        prop_assume!(from != to);
        prop_assume!(from != other);
        prop_assume!(to != other);
        let input = format!("(fn [{{:keys [{from} {other}]}}] (list {from} (fn [{from}] {from}) {other}))");
        let plan = plan_rename_binding(RenameBindingRequest {
            input: &input,
            dialect: Dialect::Clojure,
            target: RenameTarget::Path(Path::from_indexes(vec![0])),
            from: SymbolName::new(from.clone()).unwrap(),
            to: SymbolName::new(to.clone()).unwrap(),
        }).unwrap();

        SyntaxTree::parse(&plan.rewritten).unwrap();
        prop_assert!(plan.changed);
        prop_assert_eq!(plan.references.len(), 1);
        prop_assert_eq!(plan.shadowed_scope_count, 1);
        prop_assert_eq!(
            plan.rewritten,
            format!("(fn [{{{to} :{from} :keys [{other}]}}] (list {to} (fn [{from}] {from}) {other}))")
        );
    }

    #[test]
    fn pbt_rename_clojure_as_destructured_fn_parameter_remains_scope_aware(
        from in symbol_strategy(),
        to in symbol_strategy(),
        key in symbol_strategy(),
    ) {
        prop_assume!(from != to);
        prop_assume!(from != key);
        prop_assume!(to != key);
        let input = format!("(fn [{{:keys [{key}] :as {from}}}] (list {key} {from} (fn [{from}] {from})))");
        let plan = plan_rename_binding(RenameBindingRequest {
            input: &input,
            dialect: Dialect::Clojure,
            target: RenameTarget::Path(Path::from_indexes(vec![0])),
            from: SymbolName::new(from.clone()).unwrap(),
            to: SymbolName::new(to.clone()).unwrap(),
        }).unwrap();

        SyntaxTree::parse(&plan.rewritten).unwrap();
        prop_assert!(plan.changed);
        prop_assert_eq!(plan.references.len(), 1);
        prop_assert_eq!(plan.shadowed_scope_count, 1);
        prop_assert_eq!(
            plan.rewritten,
            format!("(fn [{{:keys [{key}] :as {to}}}] (list {key} {to} (fn [{from}] {from})))")
        );
    }
}
