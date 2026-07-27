#[test]
fn renames_macrolet_from_call_head() {
    let input = "(macrolet ((emit (x) x)) (emit 1))";
    let at = input.rfind("emit").expect("call");
    let plan = plan_rename_at(RenameAtRequest {
        input,
        dialect: Dialect::CommonLisp,
        at: ByteOffset::new(at),
        to: SymbolName::new("send").unwrap(),
    })
    .expect("plan");
    assert_eq!(plan.namespace, RenameAtNamespace::Macro);
    assert_eq!(plan.rewritten, "(macrolet ((send (x) x)) (send 1))");
}
#[test]
fn renames_only_selected_independent_macrolet_scope() {
    let input = "(list (macrolet ((emit (x) x)) (emit 1)) (macrolet ((emit (x) x)) (emit 2)))";
    let at = input.rfind("emit 2").expect("second call");
    let plan = plan_rename_at(RenameAtRequest {
        input,
        dialect: Dialect::CommonLisp,
        at: ByteOffset::new(at),
        to: SymbolName::new("send").unwrap(),
    })
    .expect("plan");
    assert_eq!(plan.namespace, RenameAtNamespace::Macro);
    assert_eq!(
        plan.rewritten,
        "(list (macrolet ((emit (x) x)) (emit 1)) (macrolet ((send (x) x)) (send 2)))"
    );
}

#[test]
fn renames_only_selected_independent_symbol_macrolet_scope() {
    let input =
        "(list (symbol-macrolet ((place first)) place) (symbol-macrolet ((place second)) place))";
    let at = input.rfind("place").expect("second reference");
    let plan = plan_rename_at(RenameAtRequest {
        input,
        dialect: Dialect::CommonLisp,
        at: ByteOffset::new(at),
        to: SymbolName::new("slot").unwrap(),
    })
    .expect("plan");
    assert_eq!(plan.namespace, RenameAtNamespace::Value);
    assert_eq!(
        plan.rewritten,
        "(list (symbol-macrolet ((place first)) place) (symbol-macrolet ((slot second)) slot))"
    );
}

#[test]
fn renames_symbol_macrolet_only_in_value_positions() {
    let input =
        "(symbol-macrolet ((place expansion)) (list place (place place) #'place (function place)))";
    let at = input.find("place expansion").expect("binding");
    let plan = plan_rename_at(RenameAtRequest {
        input,
        dialect: Dialect::CommonLisp,
        at: ByteOffset::new(at),
        to: SymbolName::new("slot").unwrap(),
    })
    .expect("plan");

    assert_eq!(plan.namespace, RenameAtNamespace::Value);
    assert_eq!(
        plan.rewritten,
        "(symbol-macrolet ((slot expansion)) (list slot (place slot) #'place (function place)))"
    );
}
use super::*;
