#[test]
fn renames_only_selected_independent_local_function_scope() {
    let input = "(list (flet ((work (x) x)) (work 1)) (flet ((work (x) x)) (work 2)))";
    let at = input.rfind("work 2").expect("second call");
    let plan = plan_rename_at(RenameAtRequest {
        input,
        dialect: Dialect::CommonLisp,
        at: ByteOffset::new(at),
        to: SymbolName::new("run").unwrap(),
    })
    .expect("plan");
    assert_eq!(plan.namespace, RenameAtNamespace::LocalFunction);
    assert_eq!(
        plan.rewritten,
        "(list (flet ((work (x) x)) (work 1)) (flet ((run (x) x)) (run 2)))"
    );
}

#[test]
fn renames_only_selected_independent_labels_scope() {
    let input =
        "(list (labels ((work (x) (work x))) (work 1)) (labels ((work (x) (work x))) (work 2)))";
    let at = input.rfind("work 2").expect("second call");
    let plan = plan_rename_at(RenameAtRequest {
        input,
        dialect: Dialect::CommonLisp,
        at: ByteOffset::new(at),
        to: SymbolName::new("run").unwrap(),
    })
    .expect("plan");
    assert_eq!(plan.namespace, RenameAtNamespace::LocalFunction);
    assert_eq!(
        plan.rewritten,
        "(list (labels ((work (x) (work x))) (work 1)) (labels ((run (x) (run x))) (run 2)))"
    );
}

#[test]
fn distinguishes_flet_and_labels_definition_body_scope() {
    let flet_input = "(defun work (x) x) (flet ((work (x) (work x))) (work 1))";
    let flet_body_at = flet_input.find("work x").expect("flet definition body");
    let flet_plan = plan_rename_at(RenameAtRequest {
        input: flet_input,
        dialect: Dialect::CommonLisp,
        at: ByteOffset::new(flet_body_at),
        to: SymbolName::new("outer-work").unwrap(),
    })
    .expect("flet definition body resolves to global function");
    assert_eq!(flet_plan.namespace, RenameAtNamespace::Function);
    assert_eq!(
        flet_plan.rewritten,
        "(defun outer-work (x) x) (flet ((work (x) (outer-work x))) (work 1))"
    );

    let labels_input = "(defun work (x) x) (labels ((work (x) (work x))) (work 1))";
    let labels_body_at = labels_input.find("work x").expect("labels definition body");
    let labels_plan = plan_rename_at(RenameAtRequest {
        input: labels_input,
        dialect: Dialect::CommonLisp,
        at: ByteOffset::new(labels_body_at),
        to: SymbolName::new("recur").unwrap(),
    })
    .expect("labels definition body resolves to local function");
    assert_eq!(labels_plan.namespace, RenameAtNamespace::LocalFunction);
    assert_eq!(
        labels_plan.rewritten,
        "(defun work (x) x) (labels ((recur (x) (recur x))) (recur 1))"
    );
}
use super::*;
