use super::*;

#[test]
fn skips_all_occurrences_inside_shadowing_binding_forms() {
    assert_plan(
        "(defun render () (+ (* width height) (let ((product 1)) (* width height))))",
        "0.3.1",
        true,
        1,
        1,
        "(defun render () (let ((product (* width height))) (+ product (let ((product 1)) (* width height)))))",
    );
}

#[test]
fn skips_all_occurrences_inside_package_qualified_shadowing_binding_forms() {
    assert_plan(
        "(defun render () (+ (* width height) (let ((cl:product 1)) (* width height))))",
        "0.3.1",
        true,
        1,
        1,
        "(defun render () (let ((product (* width height))) (+ product (let ((cl:product 1)) (* width height)))))",
    );
}

#[test]
fn skips_all_occurrences_inside_symbol_macrolet_shadowing_binding_forms() {
    assert_plan(
        "(defun render () (+ (* width height) (symbol-macrolet ((product 1)) (* width height))))",
        "0.3.1",
        true,
        1,
        1,
        "(defun render () (let ((product (* width height))) (+ product (symbol-macrolet ((product 1)) (* width height)))))",
    );
}

#[test]
fn skips_all_occurrences_inside_cl_user_symbol_macrolet_shadowing_binding_forms() {
    assert_plan(
        "(defun render () (+ (* width height) (cl-user:symbol-macrolet ((product 1)) (* width height))))",
        "0.3.1",
        true,
        1,
        1,
        "(defun render () (let ((product (* width height))) (+ product (cl-user:symbol-macrolet ((product 1)) (* width height)))))",
    );
}

#[test]
fn skips_all_occurrences_inside_cl_symbol_macrolet_shadowing_binding_forms() {
    assert_plan_with_dialect(
        "(defun render () (+ (* width height) (cl-symbol-macrolet ((product 1)) (* width height))))",
        Dialect::EmacsLisp,
        "0.3.1",
        true,
        1,
        1,
        "(defun render () (let ((product (* width height))) (+ product (cl-symbol-macrolet ((product 1)) (* width height)))))",
    );
}

#[test]
fn keeps_let_star_same_binding_initializer_in_outer_scope() {
    assert_plan(
        "(defun render () (+ (* width height) (let* ((product (* width height))) (* width height))))",
        "0.3.1",
        true,
        2,
        1,
        "(defun render () (let ((product (* width height))) (+ product (let* ((product product)) (* width height)))))",
    );
}

#[test]
fn skips_let_star_later_initializer_shadowed_by_previous_binding() {
    assert_plan(
        "(defun render () (+ (* width height) (let* ((product 1) (other (* width height))) (* width height))))",
        "0.3.1",
        true,
        1,
        2,
        "(defun render () (let ((product (* width height))) (+ product (let* ((product 1) (other (* width height))) (* width height)))))",
    );
}

#[test]
fn skips_all_occurrences_inside_define_setf_expander_shadowing_lambda_list() {
    assert_plan(
        "(defun render () (+ (* width height) (define-setf-expander slot (&environment product place) (* width height))))",
        "0.3.1",
        true,
        1,
        1,
        "(defun render () (let ((product (* width height))) (+ product (define-setf-expander slot (&environment product place) (* width height)))))",
    );
}

#[test]
fn skips_all_occurrences_inside_define_compiler_macro_shadowing_lambda_list() {
    assert_plan(
        "(defun render () (+ (* width height) (define-compiler-macro slot (&environment product place) (* width height))))",
        "0.3.1",
        true,
        1,
        1,
        "(defun render () (let ((product (* width height))) (+ product (define-compiler-macro slot (&environment product place) (* width height)))))",
    );
}

#[test]
fn skips_all_occurrences_inside_destructuring_bind_shadowing_body_only() {
    assert_plan(
        "(defun render () (+ (* width height) (destructuring-bind (product) (* width height) (* width height))))",
        "0.3.1",
        true,
        2,
        1,
        "(defun render () (let ((product (* width height))) (+ product (destructuring-bind (product) product (* width height)))))",
    );
}

#[test]
fn skips_all_occurrences_inside_qualified_destructuring_bind_shadowing_body_only() {
    assert_plan(
        "(defun render () (+ (* width height) (cl:destructuring-bind (product) (* width height) (* width height))))",
        "0.3.1",
        true,
        2,
        1,
        "(defun render () (let ((product (* width height))) (+ product (cl:destructuring-bind (product) product (* width height)))))",
    );
}

#[test]
fn skips_all_occurrences_inside_handler_case_shadowing_clause_only() {
    assert_plan(
        "(defun render () (+ (* width height) (handler-case (* width height) (error (product) (* width height)) (:no-error (value) (* width height)))))",
        "0.3.1",
        true,
        3,
        1,
        "(defun render () (let ((product (* width height))) (+ product (handler-case product (error (product) (* width height)) (:no-error (value) product)))))",
    );
}

#[test]
fn skips_all_occurrences_inside_qualified_handler_case_shadowing_clause_only() {
    assert_plan(
        "(defun render () (+ (* width height) (cl:handler-case (* width height) (error (product) (* width height)) (:no-error (value) (* width height)))))",
        "0.3.1",
        true,
        3,
        1,
        "(defun render () (let ((product (* width height))) (+ product (cl:handler-case product (error (product) (* width height)) (:no-error (value) product)))))",
    );
}

#[test]
fn skips_all_occurrences_inside_dolist_shadowing_result_and_body() {
    assert_plan(
        "(defun render () (+ (* width height) (dolist (product items (* width height)) (* width height))))",
        "0.3.1",
        true,
        1,
        2,
        "(defun render () (let ((product (* width height))) (+ product (dolist (product items (* width height)) (* width height)))))",
    );
}

#[test]
fn skips_all_occurrences_inside_dotimes_shadowing_result_and_body() {
    assert_plan(
        "(defun render () (+ (* width height) (dotimes (product count (* width height)) (* width height))))",
        "0.3.1",
        true,
        1,
        2,
        "(defun render () (let ((product (* width height))) (+ product (dotimes (product count (* width height)) (* width height)))))",
    );
}

#[test]
fn keeps_do_initializers_in_outer_scope_but_skips_steps_end_and_body() {
    assert_plan(
        "(defun render () (+ (* width height) (do ((product (* width height) (* width height)) (other (* width height))) ((* width height) (* width height)) (* width height))))",
        "0.3.1",
        true,
        3,
        4,
        "(defun render () (let ((product (* width height))) (+ product (do ((product product (* width height)) (other product)) ((* width height) (* width height)) (* width height)))))",
    );
}

#[test]
fn skips_do_star_later_initializers_shadowed_by_previous_binding() {
    assert_plan(
        "(defun render () (+ (* width height) (do* ((product 1) (other (* width height))) ((* width height)) (* width height))))",
        "0.3.1",
        true,
        1,
        3,
        "(defun render () (let ((product (* width height))) (+ product (do* ((product 1) (other (* width height))) ((* width height)) (* width height)))))",
    );
}

#[test]
fn keeps_prog_initializers_in_outer_scope_but_skips_body() {
    assert_plan(
        "(defun render () (+ (* width height) (prog ((product (* width height)) (other (* width height))) (* width height))))",
        "0.3.1",
        true,
        3,
        1,
        "(defun render () (let ((product (* width height))) (+ product (prog ((product product) (other product)) (* width height)))))",
    );
}

#[test]
fn skips_prog_star_later_initializers_shadowed_by_previous_binding() {
    assert_plan(
        "(defun render () (+ (* width height) (prog* ((product 1) (other (* width height))) (* width height))))",
        "0.3.1",
        true,
        1,
        2,
        "(defun render () (let ((product (* width height))) (+ product (prog* ((product 1) (other (* width height))) (* width height)))))",
    );
}

#[test]
fn skips_all_occurrences_inside_with_slots_shadowing_body_only() {
    assert_plan(
        "(defun render () (+ (* width height) (with-slots (product) panel (* width height))))",
        "0.3.1",
        true,
        1,
        1,
        "(defun render () (let ((product (* width height))) (+ product (with-slots (product) panel (* width height)))))",
    );
}

#[test]
fn skips_all_occurrences_inside_with_accessors_shadowing_body_only() {
    assert_plan(
        "(defun render () (+ (* width height) (with-accessors ((product panel-product)) panel (* width height))))",
        "0.3.1",
        true,
        1,
        1,
        "(defun render () (let ((product (* width height))) (+ product (with-accessors ((product panel-product)) panel (* width height)))))",
    );
}

#[test]
fn skips_all_occurrences_inside_macrolet_lambda_body_only() {
    assert_plan(
        "(defun render () (+ (* width height) (macrolet ((with-product (product) (* width height))) (* width height))))",
        "0.3.1",
        true,
        2,
        1,
        "(defun render () (let ((product (* width height))) (+ product (macrolet ((with-product (product) (* width height))) product))))",
    );
}

#[test]
fn skips_all_occurrences_inside_cl_macrolet_lambda_body_only() {
    assert_plan(
        "(defun render () (+ (* width height) (cl:macrolet ((with-product (product) (* width height))) (* width height))))",
        "0.3.1",
        true,
        2,
        1,
        "(defun render () (let ((product (* width height))) (+ product (cl:macrolet ((with-product (product) (* width height))) product))))",
    );
}

#[test]
fn skips_all_occurrences_inside_cl_user_compiler_macrolet_lambda_body_only() {
    assert_plan(
        "(defun render () (+ (* width height) (cl-user:compiler-macrolet ((with-product (product) (* width height))) (* width height))))",
        "0.3.1",
        true,
        2,
        1,
        "(defun render () (let ((product (* width height))) (+ product (cl-user:compiler-macrolet ((with-product (product) (* width height))) product))))",
    );
}

#[test]
fn skips_all_occurrences_inside_cl_flet_lambda_body_only() {
    assert_plan_with_dialect(
        "(defun render () (+ (* width height) (cl-flet ((with-product (product) (* width height))) (* width height))))",
        Dialect::EmacsLisp,
        "0.3.1",
        true,
        2,
        1,
        "(defun render () (let ((product (* width height))) (+ product (cl-flet ((with-product (product) (* width height))) product))))",
    );
}

#[test]
fn skips_all_occurrences_inside_cl_labels_lambda_body_only() {
    assert_plan_with_dialect(
        "(defun render () (+ (* width height) (cl-labels ((with-product (product) (* width height))) (* width height))))",
        Dialect::EmacsLisp,
        "0.3.1",
        true,
        2,
        1,
        "(defun render () (let ((product (* width height))) (+ product (cl-labels ((with-product (product) (* width height))) product))))",
    );
}
