use super::*;

#[test]
fn plans_clojure_destructured_vector_let_binding_rename() {
    let input = "(let [[value other] source next value] [value other next])";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::Clojure,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("value").unwrap(),
        to: SymbolName::new("product").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "let");
    assert_eq!(plan.references.len(), 2);
    assert_eq!(
        plan.rewritten,
        "(let [[product other] source next product] [product other next])"
    );
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn plans_clojure_destructured_fn_parameter_rename_without_shadow_capture() {
    let input = "(fn [{value :value row :row}] (list value (fn [value] value) row))";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::Clojure,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("value").unwrap(),
        to: SymbolName::new("product").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "fn");
    assert_eq!(plan.references.len(), 1);
    assert_eq!(plan.shadowed_scope_count, 1);
    assert_eq!(
        plan.rewritten,
        "(fn [{product :value row :row}] (list product (fn [value] value) row))"
    );
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn plans_clojure_keys_destructured_fn_parameter_rename_preserving_lookup_key() {
    let input = "(fn [{:keys [value row]}] (list value (fn [value] value) row))";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::Clojure,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("value").unwrap(),
        to: SymbolName::new("product").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "fn");
    assert_eq!(plan.references.len(), 1);
    assert_eq!(plan.shadowed_scope_count, 1);
    assert_eq!(
        plan.rewritten,
        "(fn [{product :value :keys [row]}] (list product (fn [value] value) row))"
    );
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn plans_clojure_keys_destructured_let_binding_rename_through_later_bindings() {
    let input = "(let [{:keys [value row]} source next value] [value row next])";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::Clojure,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("value").unwrap(),
        to: SymbolName::new("product").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "let");
    assert_eq!(plan.references.len(), 2);
    assert_eq!(
        plan.rewritten,
        "(let [{product :value :keys [row]} source next product] [product row next])"
    );
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn plans_clojure_as_destructured_fn_parameter_rename_without_shadow_capture() {
    let input = "(fn [{:keys [value] :as row}] (list value row (fn [row] row)))";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::Clojure,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("row").unwrap(),
        to: SymbolName::new("record").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "fn");
    assert_eq!(plan.references.len(), 1);
    assert_eq!(plan.shadowed_scope_count, 1);
    assert_eq!(
        plan.rewritten,
        "(fn [{:keys [value] :as record}] (list value record (fn [row] row)))"
    );
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn plans_clojure_as_destructured_let_binding_rename_through_later_bindings() {
    let input = "(let [{:keys [value] :as row} source next row] [value row next])";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::Clojure,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("row").unwrap(),
        to: SymbolName::new("record").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "let");
    assert_eq!(plan.references.len(), 2);
    assert_eq!(
        plan.rewritten,
        "(let [{:keys [value] :as record} source next record] [value record next])"
    );
    SyntaxTree::parse(&plan.rewritten).unwrap();
}
