use super::*;

#[test]
fn rejects_unknown_dialect_before_binding_rename_planning() {
    let input = "(let ((value 1)) value)";
    let error = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::Unknown,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("value").unwrap(),
        to: SymbolName::new("product").unwrap(),
    })
    .expect_err("unknown dialect should be rejected");

    assert!(
        error
            .to_string()
            .contains("rename-binding is not supported for this dialect")
    );
}

#[test]
fn plans_binding_rename_without_shadowed_inner_binding() {
    let input = "(let ((value 1)) (+ value (let ((value 2)) value) value))";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::CommonLisp,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("value").unwrap(),
        to: SymbolName::new("product").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "let");
    assert_eq!(plan.references.len(), 2);
    assert_eq!(plan.shadowed_scope_count, 1);
    assert_eq!(
        plan.rewritten,
        "(let ((product 1)) (+ product (let ((value 2)) value) product))"
    );
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn plans_binding_rename_through_global_special_declaim() {
    let input = "(let ((value 1)) (declaim (special value)) value)";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::CommonLisp,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("value").unwrap(),
        to: SymbolName::new("seed").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "let");
    assert_eq!(plan.references.len(), 2);
    assert_eq!(
        plan.rewritten,
        "(let ((seed 1)) (declaim (special seed)) seed)"
    );
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn plans_binding_rename_through_global_special_proclaim() {
    let input = "(let ((value 1)) (proclaim '(special value)) value)";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::CommonLisp,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("value").unwrap(),
        to: SymbolName::new("seed").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "let");
    assert_eq!(plan.references.len(), 2);
    assert_eq!(
        plan.rewritten,
        "(let ((seed 1)) (proclaim '(special seed)) seed)"
    );
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn plans_binding_rename_through_local_special_declaration() {
    let input = "(let ((value 1)) (declare (special value)) (declaim (special value)) (proclaim '(special value)) value)";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::CommonLisp,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("value").unwrap(),
        to: SymbolName::new("seed").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "let");
    assert_eq!(plan.references.len(), 4);
    assert_eq!(
        plan.rewritten,
        "(let ((seed 1)) (declare (special seed)) (declaim (special seed)) (proclaim '(special seed)) seed)"
    );
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn plans_binding_rename_through_locally_wrapped_special_declaration() {
    let input = "(let ((value 1)) (locally (declare (special value)) (declaim (special value)) (proclaim '(special value)) value))";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::CommonLisp,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("value").unwrap(),
        to: SymbolName::new("seed").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "let");
    assert_eq!(plan.references.len(), 4);
    assert_eq!(
        plan.rewritten,
        "(let ((seed 1)) (locally (declare (special seed)) (declaim (special seed)) (proclaim '(special seed)) seed))"
    );
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn plans_clojure_vector_binding_rename_through_later_binding_values() {
    let input = "(let [value 1 next (+ value 1)] [value next])";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::Clojure,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("value").unwrap(),
        to: SymbolName::new("seed").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "let");
    assert_eq!(plan.references.len(), 2);
    assert_eq!(plan.rewritten, "(let [seed 1 next (+ seed 1)] [seed next])");
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn plans_emacs_lisp_let_binding_rename_without_shadowed_inner_binding() {
    let input = "(let ((value 1)) (+ value (let ((value 2)) value) value))";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::EmacsLisp,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("value").unwrap(),
        to: SymbolName::new("product").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "let");
    assert_eq!(plan.references.len(), 2);
    assert_eq!(plan.shadowed_scope_count, 1);
    assert_eq!(
        plan.rewritten,
        "(let ((product 1)) (+ product (let ((value 2)) value) product))"
    );
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn plans_emacs_lisp_let_star_binding_rename_through_later_binding_values() {
    let input = "(let* ((value 1) (next (+ value 1))) (+ next value))";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::EmacsLisp,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("value").unwrap(),
        to: SymbolName::new("seed").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "let*");
    assert_eq!(plan.references.len(), 2);
    assert_eq!(
        plan.rewritten,
        "(let* ((seed 1) (next (+ seed 1))) (+ next seed))"
    );
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn plans_qualified_common_lisp_let_star_binding_rename_through_later_binding_values() {
    let input = "(cl-user:let* ((value 1) (next (+ value 1))) (+ next value))";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::CommonLisp,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("value").unwrap(),
        to: SymbolName::new("seed").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "cl-user:let*");
    assert_eq!(plan.references.len(), 2);
    assert_eq!(
        plan.rewritten,
        "(cl-user:let* ((seed 1) (next (+ seed 1))) (+ next seed))"
    );
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn plans_qualified_common_lisp_let_star_qualified_binding_name_rename_through_later_binding_values()
{
    let input = "(cl-user:let* ((cl-user:value 1) (next (+ value 1))) (+ next value))";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::CommonLisp,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("value").unwrap(),
        to: SymbolName::new("seed").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "cl-user:let*");
    assert_eq!(plan.references.len(), 2);
    assert_eq!(
        plan.rewritten,
        "(cl-user:let* ((seed 1) (next (+ seed 1))) (+ next seed))"
    );
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn plans_qualified_destructuring_bind_binding_rename() {
    let input = "(cl:destructuring-bind (value) source (list value outer))";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::CommonLisp,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("value").unwrap(),
        to: SymbolName::new("item").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "cl:destructuring-bind");
    assert_eq!(plan.references.len(), 1);
    assert_eq!(
        plan.rewritten,
        "(cl:destructuring-bind (item) source (list item outer))"
    );
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn plans_binding_rename_without_touching_qualified_destructuring_bind_body() {
    let input = "(let ((value 1)) (list value (cl:destructuring-bind (value) value value) value))";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::CommonLisp,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("value").unwrap(),
        to: SymbolName::new("seed").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "let");
    assert_eq!(plan.references.len(), 3);
    assert_eq!(plan.shadowed_scope_count, 1);
    assert_eq!(
        plan.rewritten,
        "(let ((seed 1)) (list seed (cl:destructuring-bind (value) seed value) seed))"
    );
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn plans_scheme_let_binding_rename_without_touching_shadowed_inner_binding() {
    let input = "(let ((value 1)) (+ value (let ((value 2)) value) value))";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::Scheme,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("value").unwrap(),
        to: SymbolName::new("product").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "let");
    assert_eq!(plan.references.len(), 2);
    assert_eq!(plan.shadowed_scope_count, 1);
    assert_eq!(
        plan.rewritten,
        "(let ((product 1)) (+ product (let ((value 2)) value) product))"
    );
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn plans_scheme_let_star_binding_rename_through_later_binding_values() {
    let input = "(let* ((value 1) (next (+ value 1))) (+ value next))";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::Scheme,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("value").unwrap(),
        to: SymbolName::new("seed").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "let*");
    assert_eq!(plan.references.len(), 2);
    assert_eq!(
        plan.rewritten,
        "(let* ((seed 1) (next (+ seed 1))) (+ seed next))"
    );
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn plans_scheme_lambda_parameter_rename() {
    let input = "(lambda (x y) (+ x y))";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::Scheme,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("x").unwrap(),
        to: SymbolName::new("a").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "lambda");
    assert_eq!(plan.rewritten, "(lambda (a y) (+ a y))");
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn plans_janet_vector_let_binding_rename_through_later_binding_values() {
    let input = "(let [value 1 next (+ value 1)] [value next])";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::Janet,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("value").unwrap(),
        to: SymbolName::new("seed").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "let");
    assert_eq!(plan.references.len(), 2);
    assert_eq!(plan.rewritten, "(let [seed 1 next (+ seed 1)] [seed next])");
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn plans_fennel_vector_let_binding_rename_through_later_binding_values() {
    let input = "(let [value 1 next (+ value 1)] [value next])";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::Fennel,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("value").unwrap(),
        to: SymbolName::new("seed").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "let");
    assert_eq!(plan.references.len(), 2);
    assert_eq!(plan.rewritten, "(let [seed 1 next (+ seed 1)] [seed next])");
    SyntaxTree::parse(&plan.rewritten).unwrap();
}
