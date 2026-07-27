use super::*;

#[test]
fn plans_lambda_parameter_rename_without_shadowed_inner_binding() {
    assert_binding_rename! {
        input: "(lambda (value) (list value (lambda (value) value) value))",
        dialect: Dialect::CommonLisp,
        from: "value",
        to: "product",
        form: "lambda",
        references: 2,
        shadowed_scope_count: 1,
        rewritten: "(lambda (product) (list product (lambda (value) value) product))",
    }
}

#[test]
fn plans_defun_parameter_rename() {
    assert_binding_rename! {
        input: "(defun render (value other) (list value other))",
        dialect: Dialect::CommonLisp,
        from: "value",
        to: "product",
        form: "defun",
        references: 1,
        rewritten: "(defun render (product other) (list product other))",
    }
}

#[test]
fn plans_emacs_lisp_lambda_parameter_rename_without_shadowed_inner_binding() {
    assert_binding_rename! {
        input: "(lambda (value) (list value (lambda (value) value) value))",
        dialect: Dialect::EmacsLisp,
        from: "value",
        to: "product",
        form: "lambda",
        references: 2,
        shadowed_scope_count: 1,
        rewritten: "(lambda (product) (list product (lambda (value) value) product))",
    }
}

#[test]
fn plans_emacs_lisp_defun_parameter_rename() {
    assert_binding_rename! {
        input: "(defun render (value other) (list value other))",
        dialect: Dialect::EmacsLisp,
        from: "value",
        to: "product",
        form: "defun",
        references: 1,
        rewritten: "(defun render (product other) (list product other))",
    }
}
