use super::*;

#[test]
fn plans_binding_rename_without_touching_with_slots_scope() {
    assert_binding_rename! {
        input: "(let ((value 1)) (with-slots (value (alias value)) value (list value alias)) value)",
        dialect: Dialect::CommonLisp,
        from: "value",
        to: "outer",
        form: "let",
        references: 2,
        shadowed_scope_count: 1,
        rewritten: "(let ((outer 1)) (with-slots (value (alias value)) outer (list value alias)) outer)",
    }
}

#[test]
fn plans_binding_rename_without_touching_with_accessors_scope() {
    assert_binding_rename! {
        input: "(let ((value 1)) (with-accessors ((value get-value) (alias value)) value (list value alias)) value)",
        dialect: Dialect::CommonLisp,
        from: "value",
        to: "outer",
        form: "let",
        references: 2,
        shadowed_scope_count: 1,
        rewritten: "(let ((outer 1)) (with-accessors ((value get-value) (alias value)) outer (list value alias)) outer)",
    }
}

#[test]
fn plans_with_slots_binding_rename_preserves_bare_slot_name() {
    assert_binding_rename! {
        input: "(with-slots (value (alias slot-name)) object (list value alias object))",
        dialect: Dialect::CommonLisp,
        from: "value",
        to: "slot-value",
        form: "with-slots",
        references: 1,
        rewritten: "(with-slots ((slot-value value) (alias slot-name)) object (list slot-value alias object))",
    }
}

#[test]
fn plans_qualified_with_slots_binding_rename_preserves_bare_slot_name() {
    assert_binding_rename! {
        input: "(cl-user:with-slots (value (alias slot-name)) object (list value alias object))",
        dialect: Dialect::CommonLisp,
        from: "value",
        to: "slot-value",
        form: "cl-user:with-slots",
        references: 1,
        rewritten: "(cl-user:with-slots ((slot-value value) (alias slot-name)) object (list slot-value alias object))",
    }
}

#[test]
fn plans_qualified_with_slots_qualified_binding_name_rename_preserves_bare_slot_name() {
    assert_binding_rename! {
        input: "(cl-user:with-slots (cl-user:value (alias slot-name)) object (list value alias object))",
        dialect: Dialect::CommonLisp,
        from: "value",
        to: "slot-value",
        form: "cl-user:with-slots",
        references: 1,
        rewritten: "(cl-user:with-slots ((slot-value cl-user:value) (alias slot-name)) object (list slot-value alias object))",
    }
}

#[test]
fn plans_with_accessors_binding_rename_preserves_accessor_name() {
    assert_binding_rename! {
        input: "(with-accessors ((value get-value) (alias get-alias)) object (list value alias object))",
        dialect: Dialect::CommonLisp,
        from: "value",
        to: "slot-value",
        form: "with-accessors",
        references: 1,
        rewritten: "(with-accessors ((slot-value get-value) (alias get-alias)) object (list slot-value alias object))",
    }
}

#[test]
fn plans_qualified_with_accessors_binding_rename_preserves_accessor_name() {
    assert_binding_rename! {
        input: "(cl-user:with-accessors ((value get-value) (alias get-alias)) object (list value alias object))",
        dialect: Dialect::CommonLisp,
        from: "value",
        to: "slot-value",
        form: "cl-user:with-accessors",
        references: 1,
        rewritten: "(cl-user:with-accessors ((slot-value get-value) (alias get-alias)) object (list slot-value alias object))",
    }
}

#[test]
fn rejects_ambiguous_with_slots_binding_rename() {
    let input = "(with-slots (value (value slot-name)) object value)";
    let error = plan_rename_binding(RenameBindingRequest {
        input,
        dialect: Dialect::CommonLisp,
        target: RenameTarget::Path(Path::from_indexes(vec![0])),
        from: SymbolName::new("value").unwrap(),
        to: SymbolName::new("slot-value").unwrap(),
    })
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("binding 'value' was found in multiple selected with-slots specs")
    );
}
