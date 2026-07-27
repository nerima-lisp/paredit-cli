use super::*;

#[test]
fn plans_binding_rename_without_touching_do_scope() {
    assert_binding_rename! {
        input: "(let ((value 1)) (list value (do ((value value (1+ value))) ((done) value) value) value))",
        dialect: Dialect::CommonLisp,
        from: "value",
        to: "outer",
        form: "let",
        references: 3,
        shadowed_scope_count: 1,
        rewritten: "(let ((outer 1)) (list outer (do ((value outer (1+ value))) ((done) value) value) outer))",
    }
}

#[test]
fn plans_binding_rename_without_touching_prog_scope() {
    assert_binding_rename! {
        input: "(let ((value 1)) (list value (prog ((value value) (copy value)) (return value)) value))",
        dialect: Dialect::CommonLisp,
        from: "value",
        to: "outer",
        form: "let",
        references: 4,
        shadowed_scope_count: 1,
        rewritten: "(let ((outer 1)) (list outer (prog ((value outer) (copy outer)) (return value)) outer))",
    }
}

#[test]
fn plans_do_binding_rename_across_steps_end_clause_and_body() {
    assert_binding_rename! {
        input: "(do ((value seed (1+ value))) ((done value) value) (collect value seed))",
        dialect: Dialect::CommonLisp,
        from: "value",
        to: "item",
        form: "do",
        references: 4,
        rewritten: "(do ((item seed (1+ item))) ((done item) item) (collect item seed))",
    }
}

#[test]
fn plans_qualified_do_binding_rename_across_steps_end_clause_and_body() {
    assert_binding_rename! {
        input: "(cl-user:do ((value seed (1+ value))) ((done value) value) (collect value seed))",
        dialect: Dialect::CommonLisp,
        from: "value",
        to: "item",
        form: "cl-user:do",
        references: 4,
        rewritten: "(cl-user:do ((item seed (1+ item))) ((done item) item) (collect item seed))",
    }
}

#[test]
fn plans_qualified_do_star_binding_rename_across_later_inits_and_body() {
    assert_binding_rename! {
        input: "(cl-user:do* ((value seed (1+ value)) (copy value)) ((done value) value) (collect value copy))",
        dialect: Dialect::CommonLisp,
        from: "value",
        to: "item",
        form: "cl-user:do*",
        references: 5,
        rewritten: "(cl-user:do* ((item seed (1+ item)) (copy item)) ((done item) item) (collect item copy))",
    }
}

#[test]
fn plans_prog_star_binding_rename_across_later_inits_and_body() {
    assert_binding_rename! {
        input: "(prog* ((value seed) (copy value)) (return (list value copy)))",
        dialect: Dialect::CommonLisp,
        from: "value",
        to: "item",
        form: "prog*",
        references: 2,
        rewritten: "(prog* ((item seed) (copy item)) (return (list item copy)))",
    }
}

#[test]
fn plans_qualified_prog_star_binding_rename_across_later_inits_and_body() {
    assert_binding_rename! {
        input: "(cl-user:prog* ((value seed) (copy value)) (return (list value copy)))",
        dialect: Dialect::CommonLisp,
        from: "value",
        to: "item",
        form: "cl-user:prog*",
        references: 2,
        rewritten: "(cl-user:prog* ((item seed) (copy item)) (return (list item copy)))",
    }
}
