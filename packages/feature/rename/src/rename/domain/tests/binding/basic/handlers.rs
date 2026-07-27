use super::*;

#[test]
fn plans_handler_bind_lambda_parameter_rename_without_touching_outer_scope() {
    let input =
        "(handler-bind ((error (lambda (condition) (recover condition outer)))) (log condition))";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::CommonLisp,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("condition").unwrap(),
        to: SymbolName::new("err").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "handler-bind");
    assert_eq!(plan.references.len(), 1);
    assert_eq!(
        plan.rewritten,
        "(handler-bind ((error (lambda (err) (recover err outer)))) (log condition))"
    );
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn plans_restart_bind_lambda_parameter_rename_without_touching_other_handler_functions() {
    let input = "(restart-bind ((retry (lambda (condition) (recover condition)) :report (lambda (stream) stream))) (notify condition))";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::CommonLisp,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("condition").unwrap(),
        to: SymbolName::new("reason").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "restart-bind");
    assert_eq!(plan.references.len(), 1);
    assert_eq!(
        plan.rewritten,
        "(restart-bind ((retry (lambda (reason) (recover reason)) :report (lambda (stream) stream))) (notify condition))"
    );
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn rejects_ambiguous_handler_bind_lambda_parameter_rename() {
    let input = "(handler-bind ((error (lambda (condition) condition)) (warning (lambda (condition) condition))) (signal condition))";
    let error = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::CommonLisp,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("condition").unwrap(),
        to: SymbolName::new("err").unwrap(),
    })
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("multiple selected handler-bind handler functions")
    );
}

#[test]
fn plans_qualified_handler_bind_lambda_parameter_rename() {
    let input = "(cl-user:handler-bind ((error (lambda (condition) (recover condition outer)))) (log condition))";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::CommonLisp,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("condition").unwrap(),
        to: SymbolName::new("err").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "cl-user:handler-bind");
    assert_eq!(plan.references.len(), 1);
    assert_eq!(
        plan.rewritten,
        "(cl-user:handler-bind ((error (lambda (err) (recover err outer)))) (log condition))"
    );
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn plans_qualified_handler_bind_qualified_lambda_parameter_rename() {
    let input = "(cl-user:handler-bind ((error (lambda (cl-user:condition) (recover condition outer)))) (log condition))";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::CommonLisp,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("condition").unwrap(),
        to: SymbolName::new("err").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "cl-user:handler-bind");
    assert_eq!(plan.references.len(), 1);
    assert_eq!(
        plan.rewritten,
        "(cl-user:handler-bind ((error (lambda (err) (recover err outer)))) (log condition))"
    );
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn plans_qualified_restart_bind_lambda_parameter_rename() {
    let input = "(cl-user:restart-bind ((retry (lambda (condition) (recover condition)) :report (lambda (stream) stream))) (notify condition))";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::CommonLisp,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("condition").unwrap(),
        to: SymbolName::new("reason").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "cl-user:restart-bind");
    assert_eq!(plan.references.len(), 1);
    assert_eq!(
        plan.rewritten,
        "(cl-user:restart-bind ((retry (lambda (reason) (recover reason)) :report (lambda (stream) stream))) (notify condition))"
    );
    SyntaxTree::parse(&plan.rewritten).unwrap();
}
