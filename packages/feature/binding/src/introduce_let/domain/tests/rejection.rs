use super::*;

#[test]
fn rejects_unknown_dialect_before_introduce_let_planning() {
    let mut request = request("(+ width height)", "0.1", false);
    request.dialect = Dialect::Unknown;

    let error = plan_introduce_let(request).expect_err("unknown dialect should be rejected");

    assert!(
        error
            .to_string()
            .contains("introduce-let is not supported for this dialect")
    );
}

#[test]
fn rejects_selected_expression_inside_shadowing_binding_form() {
    assert_shadowed_error(
        "(defun render () (let ((product 1)) (* width height)))",
        "0.3.2",
    );
}

#[test]
fn rejects_selected_expression_inside_package_qualified_shadowing_binding_form() {
    assert_shadowed_error(
        "(defun render () (let ((cl:product 1)) (* width height)))",
        "0.3.2",
    );
}

#[test]
fn rejects_selected_expression_inside_symbol_macrolet_shadowing_binding_form() {
    assert_shadowed_error(
        "(defun render () (symbol-macrolet ((product 1)) (* width height)))",
        "0.3.2",
    );
}

#[test]
fn rejects_selected_expression_inside_cl_user_symbol_macrolet_shadowing_binding_form() {
    assert_shadowed_error(
        "(defun render () (cl-user:symbol-macrolet ((product 1)) (* width height)))",
        "0.3.2",
    );
}

#[test]
fn rejects_selected_expression_inside_define_setf_expander_shadowing_lambda_list() {
    assert_shadowed_error(
        "(defun render () (define-setf-expander slot (&environment product place) (* width height)))",
        "0.3.3",
    );
}

#[test]
fn rejects_selected_expression_inside_define_compiler_macro_shadowing_lambda_list() {
    assert_shadowed_error(
        "(defun render () (define-compiler-macro slot (&environment product place) (* width height)))",
        "0.3.3",
    );
}

#[test]
fn rejects_selected_expression_inside_destructuring_bind_shadowing_body() {
    assert_shadowed_error(
        "(defun render () (destructuring-bind (product) row (* width height)))",
        "0.3.3",
    );
}

#[test]
fn rejects_selected_expression_inside_handler_case_shadowing_clause() {
    assert_shadowed_error(
        "(defun render () (handler-case (risky) (error (product) (* width height))))",
        "0.3.2.2",
    );
}

#[test]
fn rejects_selected_expression_inside_dolist_shadowing_result() {
    assert_shadowed_error(
        "(defun render () (dolist (product items (* width height)) (done)))",
        "0.3.1.2",
    );
}

#[test]
fn rejects_selected_expression_inside_do_step_shadowing_binding() {
    assert_shadowed_error(
        "(defun render () (do ((product 1 (* width height))) ((done)) (finish)))",
        "0.3.1.0.2",
    );
}

#[test]
fn rejects_selected_expression_inside_prog_body_shadowing_binding() {
    assert_shadowed_error(
        "(defun render () (prog ((product 1)) (* width height)))",
        "0.3.2",
    );
}

#[test]
fn rejects_selected_expression_inside_with_slots_shadowing_body() {
    assert_shadowed_error(
        "(defun render () (with-slots (product) panel (* width height)))",
        "0.3.3",
    );
}

#[test]
fn rejects_selected_expression_inside_macrolet_lambda_body_shadowing_parameter() {
    assert_shadowed_error(
        "(defun render () (macrolet ((with-product (product) (* width height))) (done)))",
        "0.3.1.0.2",
    );
}

#[test]
fn rejects_selected_expression_inside_cl_macrolet_lambda_body_shadowing_parameter() {
    assert_shadowed_error(
        "(defun render () (cl:macrolet ((with-product (product) (* width height))) (done)))",
        "0.3.1.0.2",
    );
}

#[test]
fn rejects_selected_expression_inside_cl_user_compiler_macrolet_lambda_body_shadowing_parameter() {
    assert_shadowed_error(
        "(defun render () (cl-user:compiler-macrolet ((with-product (product) (* width height))) (done)))",
        "0.3.1.0.2",
    );
}
