use super::*;

#[test]
fn plans_labels_lambda_list_parameter_rename_without_touching_outer_body_call() {
    assert_binding_rename! {
        input: "(labels ((walk (value) (if value (walk value) value))) (walk seed) value)",
        dialect: Dialect::CommonLisp,
        from: "value",
        to: "node",
        form: "labels",
        references: 3,
        rewritten: "(labels ((walk (node) (if node (walk node) node))) (walk seed) value)",
    }
}

#[test]
fn rejects_ambiguous_labels_local_callable_lambda_list_parameter_rename() {
    let input = "(labels ((left (value) value) (right (value) value)) (left 1))";
    let error = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::CommonLisp,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("value").unwrap(),
        to: SymbolName::new("node").unwrap(),
    })
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("multiple selected labels local callable lambda lists")
    );
}

#[test]
fn plans_emacs_lisp_cl_labels_lambda_list_parameter_rename_without_touching_outer_body_call() {
    assert_binding_rename! {
        input: "(cl-labels ((walk (value) (if value (walk value) value))) (walk seed) value)",
        dialect: Dialect::EmacsLisp,
        from: "value",
        to: "node",
        form: "cl-labels",
        references: 3,
        rewritten: "(cl-labels ((walk (node) (if node (walk node) node))) (walk seed) value)",
    }
}
