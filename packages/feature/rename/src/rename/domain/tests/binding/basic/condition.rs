use super::*;

#[test]
fn plans_handler_case_clause_parameter_rename_without_touching_other_scopes() {
    let input = "(handler-case (risky condition) (error (condition) (recover condition outer)) (:no-error (value) (finish value condition)))";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::CommonLisp,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("condition").unwrap(),
        to: SymbolName::new("err").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "handler-case");
    assert_eq!(plan.references.len(), 1);
    assert_eq!(
        plan.rewritten,
        "(handler-case (risky condition) (error (err) (recover err outer)) (:no-error (value) (finish value condition)))"
    );
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn plans_restart_case_clause_parameter_rename_without_crossing_nested_clause_shadow() {
    let input = "(restart-case (risky condition) (retry (condition) (recover condition (handler-case (again) (error (condition) condition)))) (skip () condition))";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::CommonLisp,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("condition").unwrap(),
        to: SymbolName::new("reason").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "restart-case");
    assert_eq!(plan.references.len(), 1);
    assert_eq!(plan.shadowed_scope_count, 1);
    assert_eq!(
        plan.rewritten,
        "(restart-case (risky condition) (retry (reason) (recover reason (handler-case (again) (error (condition) condition)))) (skip () condition))"
    );
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn rejects_ambiguous_handler_case_clause_parameter_rename() {
    let input =
        "(handler-case (risky) (error (condition) condition) (warning (condition) condition))";
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
            .contains("multiple selected handler-case clauses")
    );
}

#[test]
fn plans_qualified_handler_case_clause_parameter_rename_without_touching_other_scopes() {
    let input = "(cl-user:handler-case (risky condition) (error (condition) (recover condition outer)) (:no-error (value) (finish value condition)))";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::CommonLisp,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("condition").unwrap(),
        to: SymbolName::new("err").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "cl-user:handler-case");
    assert_eq!(plan.references.len(), 1);
    assert_eq!(
        plan.rewritten,
        "(cl-user:handler-case (risky condition) (error (err) (recover err outer)) (:no-error (value) (finish value condition)))"
    );
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn plans_qualified_restart_case_clause_parameter_rename_without_crossing_nested_clause_shadow() {
    let input = "(cl-user:restart-case (risky condition) (retry (condition) (recover condition (handler-case (again) (error (condition) condition)))) (skip () condition))";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::CommonLisp,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("condition").unwrap(),
        to: SymbolName::new("reason").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "cl-user:restart-case");
    assert_eq!(plan.references.len(), 1);
    assert_eq!(plan.shadowed_scope_count, 1);
    assert_eq!(
        plan.rewritten,
        "(cl-user:restart-case (risky condition) (retry (reason) (recover reason (handler-case (again) (error (condition) condition)))) (skip () condition))"
    );
    SyntaxTree::parse(&plan.rewritten).unwrap();
}
