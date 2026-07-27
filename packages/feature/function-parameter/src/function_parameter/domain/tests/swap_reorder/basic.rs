use super::*;

#[test]
fn swaps_parameters_and_call_arguments() {
    let input = "(defun f (a b c) (list a b c))\n(print (f 1 2 3))";
    let plan = plan_swap_function_parameters(SwapFunctionParametersRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        left_name: symbol("a"),
        right_name: symbol("c"),
        call_paths: vec![path("1.1")],
        all_calls: false,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defun f (c b a) (list a b c))\n(print (f 3 2 1))"
    );
    assert_eq!(plan.left_index, 0);
    assert_eq!(plan.right_index, 2);
    assert_swapped_arguments(&plan.swapped_arguments, &[("1", "3")]);
}

#[test]
fn reorders_parameters_and_call_arguments() {
    let input = "(defun f (a b c) (list a b c))\n(print (f 1 2 3))";
    let plan = plan_reorder_function_parameters(ReorderFunctionParametersRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        parameter_order: symbol_names(&["c", "a", "b"]),
        call_paths: vec![path("1.1")],
        all_calls: false,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defun f (c a b) (list a b c))\n(print (f 3 1 2))"
    );
    assert_parameter_order(&plan.old_parameter_order, &["a", "b", "c"]);
    assert_parameter_order(&plan.new_parameter_order, &["c", "a", "b"]);
    assert_reordered_arguments(&plan.reordered_arguments, &[&["3", "1", "2"]]);
}

#[test]
fn rejects_reorder_with_missing_parameter() {
    let input = "(defun f (a b c) (list a b c))";
    let error = plan_reorder_function_parameters(ReorderFunctionParametersRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        parameter_order: symbol_names(&["c", "a"]),
        call_paths: Vec::new(),
        all_calls: false,
    })
    .expect_err("missing parameter must fail");

    assert!(error.to_string().contains("definition has 3"));
}
