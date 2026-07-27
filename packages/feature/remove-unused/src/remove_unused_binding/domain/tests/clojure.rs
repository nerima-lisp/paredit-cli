use super::*;

#[test]
fn plans_clojure_vector_binding() {
    let input = "(let [unused 1 used 2] used)";
    let plan = plan_remove_unused_binding(RemoveUnusedBindingRequest {
        input,
        dialect: Dialect::Clojure,
        path: None,
        target: target(input),
        name: Some(&SymbolName::new("unused").expect("symbol")),
        all_bindings: false,
        allow_drop_value: true,
    })
    .expect("plan");

    assert_eq!(plan.binding_name.as_deref(), Some("unused"));
    assert_eq!(plan.replacement, "(let [used 2]\n  used)");
    assert_eq!(plan.rewritten, "(let [used 2]\n  used)");
}

#[test]
fn plans_clojure_vector_unused_binding_ignoring_shadowed_fn_parameter() {
    let input = "(let [x 1 used 2] (list used (fn [x] x)))";
    let plan = plan_remove_unused_binding(RemoveUnusedBindingRequest {
        input,
        dialect: Dialect::Clojure,
        path: None,
        target: target(input),
        name: Some(&SymbolName::new("x").expect("symbol")),
        all_bindings: false,
        allow_drop_value: true,
    })
    .expect("plan");

    assert_eq!(plan.binding_name.as_deref(), Some("x"));
    assert_eq!(plan.reference_count, Some(0));
    assert_eq!(plan.replacement, "(let [used 2]\n  (list used (fn [x] x)))");
}
