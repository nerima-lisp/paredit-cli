use super::*;

#[test]
fn rejects_swap_parameter_across_common_lisp_lambda_list_section() {
    let input = "(defun f (a &optional b) (list a b))\n(print (f 1 2))";
    let error = plan_swap_function_parameters(SwapFunctionParametersRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        left_name: symbol("a"),
        right_name: symbol("b"),
        call_paths: vec![path("1.1")],
        all_calls: false,
    })
    .expect_err("section crossing must fail");

    assert!(
        error
            .to_string()
            .contains("cannot move 'b' across Common Lisp lambda-list sections")
    );
    assert!(
        error.to_string().starts_with("swap-function-parameters"),
        "swap error must name its own command, got: {error}"
    );
}

#[test]
fn rejects_swap_when_the_parameter_list_contains_a_comment() {
    let input =
        "(defun f (a\n          ;; b is the divisor\n          b) (list a b))\n(print (f 1 2))";
    let error = plan_swap_function_parameters(SwapFunctionParametersRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        left_name: symbol("a"),
        right_name: symbol("b"),
        call_paths: vec![path("1.1")],
        all_calls: false,
    })
    .expect_err("a comment in the parameter list must not be silently discarded");

    assert!(error.to_string().contains("comment"));
}

#[test]
fn rejects_reorder_when_the_parameter_list_contains_a_comment() {
    let input = "(defun f (a b\n          ;; c is optional context\n          c) (list a b c))\n(print (f 1 2 3))";
    let error = plan_reorder_function_parameters(ReorderFunctionParametersRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        parameter_order: symbol_names(&["c", "a", "b"]),
        call_paths: vec![path("1.1")],
        all_calls: false,
    })
    .expect_err("a comment in the parameter list must not be silently discarded");

    assert!(error.to_string().contains("comment"));
}
