#[test]
fn renames_value_binding_from_reference() {
    let input = "(let ((value 1)) (+ value value))";
    let plan = plan_rename_at(RenameAtRequest {
        input,
        dialect: Dialect::CommonLisp,
        at: ByteOffset::new(input.rfind("value").unwrap()),
        to: SymbolName::new("count").unwrap(),
    })
    .expect("plan");
    assert_eq!(plan.namespace, RenameAtNamespace::Value);
    assert_eq!(plan.rewritten, "(let ((count 1)) (+ count count))");
}

#[test]
fn keeps_common_lisp_value_and_function_namespaces_separate() {
    let input = "(let ((value 1)) (list value (value)))";
    let plan = plan_rename_at(request(input, "value 1", "item")).expect("plan");
    assert_eq!(plan.rewritten, "(let ((item 1)) (list item (value)))");
}

#[test]
fn renames_macro_lambda_binding_as_a_value() {
    let input = "(defmacro emit (&whole whole &environment environment form) `(list ,whole ,environment ,form))";
    let plan =
        plan_rename_at(request(input, "environment form", "expansion-environment")).expect("plan");
    assert_eq!(plan.namespace, RenameAtNamespace::Value);
    assert_eq!(
        plan.rewritten,
        "(defmacro emit (&whole whole &environment expansion-environment form) `(list ,whole ,expansion-environment ,form))"
    );
}
use super::*;
