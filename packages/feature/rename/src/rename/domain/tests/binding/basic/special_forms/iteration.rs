use super::*;

#[test]
fn plans_binding_rename_without_touching_dolist_iteration_scope() {
    assert_binding_rename! {
        input: "(let ((value 1) (items 2)) (list value (dolist (value items value) value) value))",
        dialect: Dialect::CommonLisp,
        from: "value",
        to: "seed",
        form: "let",
        references: 2,
        shadowed_scope_count: 1,
        rewritten: "(let ((seed 1) (items 2)) (list seed (dolist (value items value) value) seed))",
    }
}

#[test]
fn plans_dolist_iteration_binding_rename_without_touching_source() {
    assert_binding_rename! {
        input: "(dolist (value items value) (collect value) items)",
        dialect: Dialect::CommonLisp,
        from: "value",
        to: "item",
        form: "dolist",
        references: 2,
        rewritten: "(dolist (item items item) (collect item) items)",
    }
}

#[test]
fn plans_qualified_dolist_iteration_binding_rename_without_touching_source() {
    assert_binding_rename! {
        input: "(cl-user:dolist (value items value) (collect value) items)",
        dialect: Dialect::CommonLisp,
        from: "value",
        to: "item",
        form: "cl-user:dolist",
        references: 2,
        rewritten: "(cl-user:dolist (item items item) (collect item) items)",
    }
}

#[test]
fn plans_qualified_dolist_qualified_binding_name_rename_without_touching_source() {
    assert_binding_rename! {
        input: "(cl-user:dolist (cl-user:value items value) (collect value) items)",
        dialect: Dialect::CommonLisp,
        from: "value",
        to: "item",
        form: "cl-user:dolist",
        references: 2,
        rewritten: "(cl-user:dolist (item items item) (collect item) items)",
    }
}

#[test]
fn plans_dotimes_iteration_binding_rename_without_touching_count() {
    assert_binding_rename! {
        input: "(dotimes (index limit index) (push index result) limit)",
        dialect: Dialect::CommonLisp,
        from: "index",
        to: "i",
        form: "dotimes",
        references: 2,
        rewritten: "(dotimes (i limit i) (push i result) limit)",
    }
}

#[test]
fn plans_qualified_dotimes_iteration_binding_rename_without_touching_count() {
    assert_binding_rename! {
        input: "(cl-user:dotimes (index limit index) (push index result) limit)",
        dialect: Dialect::CommonLisp,
        from: "index",
        to: "i",
        form: "cl-user:dotimes",
        references: 2,
        rewritten: "(cl-user:dotimes (i limit i) (push i result) limit)",
    }
}
