use super::*;

#[test]
fn removes_parameter_and_call_argument() {
    let input = "(defun f (a b) (+ a b))\n(print (f 1 2))";
    let plan = plan_remove_function_parameter(RemoveFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("b"),
        call_paths: vec![path("1.1")],
        all_calls: false,
        missing_argument_policy: MissingArgumentPolicy::Reject,
    })
    .expect("plan");

    assert_eq!(plan.rewritten, "(defun f (a) (+ a b))\n(print (f 1))");
    assert_eq!(plan.removed_arguments, vec![Some("2".to_owned())]);
}

#[test]
fn removing_the_first_parameter_keeps_the_next_parameters_own_comment() {
    let input =
        "(defun f (a\n          ;; b is optional context\n          b) (+ a b))\n(print (f 1 2))";
    let plan = plan_remove_function_parameter(RemoveFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("a"),
        call_paths: vec![path("1.1")],
        all_calls: false,
        missing_argument_policy: MissingArgumentPolicy::Reject,
    })
    .expect("plan");

    assert!(
        plan.rewritten.contains(";; b is optional context"),
        "comment describing the surviving parameter must not be discarded: {:?}",
        plan.rewritten
    );
    assert_eq!(plan.removed_arguments, vec![Some("1".to_owned())]);
}

#[test]
fn removes_unqualified_name_for_package_qualified_common_lisp_parameter() {
    let input = "(defun f (cl:stream other) (+ stream other))\n(print (f 1 2))";
    let plan = plan_remove_function_parameter(RemoveFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("stream"),
        call_paths: vec![path("1.1")],
        all_calls: false,
        missing_argument_policy: MissingArgumentPolicy::Reject,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defun f (other) (+ stream other))\n(print (f 2))"
    );
    assert_eq!(plan.parameter_index, 0);
    assert_eq!(plan.removed_arguments, vec![Some("1".to_owned())]);
}
