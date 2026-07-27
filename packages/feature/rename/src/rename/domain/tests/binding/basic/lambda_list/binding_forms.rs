use super::*;

#[test]
fn plans_destructuring_bind_rename_without_touching_value_form() {
    assert_binding_rename! {
        input: "(destructuring-bind (value other) (parse value) (list value other))",
        dialect: Dialect::CommonLisp,
        from: "value",
        to: "slot",
        form: "destructuring-bind",
        references: 1,
        rewritten: "(destructuring-bind (slot other) (parse value) (list slot other))",
    }
}

#[test]
fn plans_multiple_value_bind_rename_without_shadowed_inner_binding() {
    assert_binding_rename! {
        input: "(multiple-value-bind (value other) (compute) (list value (destructuring-bind (value) row value) other value))",
        dialect: Dialect::CommonLisp,
        from: "value",
        to: "slot",
        form: "multiple-value-bind",
        references: 2,
        shadowed_scope_count: 1,
        rewritten: "(multiple-value-bind (slot other) (compute) (list slot (destructuring-bind (value) row value) other slot))",
    }
}

#[test]
fn plans_multiple_value_bind_qualified_binding_name_rename() {
    assert_binding_rename! {
        input: "(multiple-value-bind (cl-user:value other) (compute) (list value other value))",
        dialect: Dialect::CommonLisp,
        from: "value",
        to: "slot",
        form: "multiple-value-bind",
        references: 2,
        rewritten: "(multiple-value-bind (slot other) (compute) (list slot other slot))",
    }
}
