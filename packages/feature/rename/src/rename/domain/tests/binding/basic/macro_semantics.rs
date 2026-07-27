use super::*;

#[test]
fn plans_symbol_macrolet_binding_rename_without_touching_expansion_reference() {
    let input = "(symbol-macrolet ((value (compute value))) (list value value))";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::CommonLisp,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("value").unwrap(),
        to: SymbolName::new("slot").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "symbol-macrolet");
    assert_eq!(plan.references.len(), 2);
    assert_eq!(
        plan.rewritten,
        "(symbol-macrolet ((slot (compute value))) (list slot slot))"
    );
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn plans_emacs_lisp_cl_symbol_macrolet_binding_rename_without_touching_expansion_reference() {
    let input = "(cl-symbol-macrolet ((value (compute value))) (list value value))";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::EmacsLisp,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("value").unwrap(),
        to: SymbolName::new("slot").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "cl-symbol-macrolet");
    assert_eq!(plan.references.len(), 2);
    assert_eq!(
        plan.rewritten,
        "(cl-symbol-macrolet ((slot (compute value))) (list slot slot))"
    );
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn plans_binding_rename_inside_quasiquote_preserving_unquote_prefixes() {
    let input = "(let ((value items)) `(,value ,@value value))";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::CommonLisp,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("value").unwrap(),
        to: SymbolName::new("forms").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "let");
    assert_eq!(plan.references.len(), 2);
    assert_eq!(
        plan.rewritten,
        "(let ((forms items)) `(,forms ,@forms value))"
    );
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn plans_binding_rename_only_after_matching_nested_unquote_depth() {
    let input = "(let ((value 1)) `(outer `,value ,,value value))";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::CommonLisp,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("value").unwrap(),
        to: SymbolName::new("item").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "let");
    assert_eq!(plan.references.len(), 1);
    assert_eq!(
        plan.rewritten,
        "(let ((item 1)) `(outer `,value ,,item value))"
    );
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn plans_outer_binding_rename_through_symbol_macrolet_expansion_only() {
    let input =
        "(let ((value 1)) (list value (symbol-macrolet ((value (compute value))) value) value))";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::CommonLisp,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("value").unwrap(),
        to: SymbolName::new("outer").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "let");
    assert_eq!(plan.references.len(), 3);
    assert_eq!(plan.shadowed_scope_count, 1);
    assert_eq!(
        plan.rewritten,
        "(let ((outer 1)) (list outer (symbol-macrolet ((value (compute outer))) value) outer))"
    );
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn plans_emacs_lisp_outer_binding_rename_through_cl_symbol_macrolet_expansion_only() {
    let input =
        "(let ((value 1)) (list value (cl-symbol-macrolet ((value (compute value))) value) value))";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::EmacsLisp,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("value").unwrap(),
        to: SymbolName::new("outer").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "let");
    assert_eq!(plan.references.len(), 3);
    assert_eq!(plan.shadowed_scope_count, 1);
    assert_eq!(
        plan.rewritten,
        "(let ((outer 1)) (list outer (cl-symbol-macrolet ((value (compute outer))) value) outer))"
    );
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn plans_outer_binding_rename_through_macrolet_expander_only() {
    let input = "(let ((value 1)) (list value (macrolet ((emit () value)) (emit) value) value))";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::CommonLisp,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("value").unwrap(),
        to: SymbolName::new("outer").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "let");
    assert_eq!(plan.references.len(), 4);
    assert_eq!(plan.shadowed_scope_count, 0);
    assert_eq!(
        plan.rewritten,
        "(let ((outer 1)) (list outer (macrolet ((emit () outer)) (emit) outer) outer))"
    );
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn plans_outer_binding_rename_through_cl_user_macrolet_expander_only() {
    let input =
        "(let ((value 1)) (list value (cl-user:macrolet ((emit () value)) (emit) value) value))";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::CommonLisp,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("value").unwrap(),
        to: SymbolName::new("outer").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "let");
    assert_eq!(plan.references.len(), 4);
    assert_eq!(plan.shadowed_scope_count, 0);
    assert_eq!(
        plan.rewritten,
        "(let ((outer 1)) (list outer (cl-user:macrolet ((emit () outer)) (emit) outer) outer))"
    );
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn plans_outer_binding_rename_through_compiler_macrolet_expander_only() {
    let input =
        "(let ((value 1)) (list value (compiler-macrolet ((emit () value)) (emit) value) value))";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::CommonLisp,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("value").unwrap(),
        to: SymbolName::new("outer").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "let");
    assert_eq!(plan.references.len(), 4);
    assert_eq!(plan.shadowed_scope_count, 0);
    assert_eq!(
        plan.rewritten,
        "(let ((outer 1)) (list outer (compiler-macrolet ((emit () outer)) (emit) outer) outer))"
    );
    SyntaxTree::parse(&plan.rewritten).unwrap();
}

#[test]
fn plans_outer_binding_rename_through_cl_user_compiler_macrolet_expander_only() {
    let input = "(let ((value 1)) (list value (cl-user:compiler-macrolet ((emit () value)) (emit) value) value))";
    let plan = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::CommonLisp,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("value").unwrap(),
        to: SymbolName::new("outer").unwrap(),
    })
    .unwrap();

    assert_eq!(plan.form, "let");
    assert_eq!(plan.references.len(), 4);
    assert_eq!(plan.shadowed_scope_count, 0);
    assert_eq!(
        plan.rewritten,
        "(let ((outer 1)) (list outer (cl-user:compiler-macrolet ((emit () outer)) (emit) outer) outer))"
    );
    SyntaxTree::parse(&plan.rewritten).unwrap();
}
