use super::*;

#[test]
fn rejects_common_lisp_optional_parameter_when_call_argument_is_missing_under_strict_policy() {
    let input = "(defun f (a &optional (b 2 b-p) c) (list a b c))\n(print (f 1))";
    let error = plan_remove_function_parameter(RemoveFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("b"),
        call_paths: vec![path("1.1")],
        all_calls: false,
        missing_argument_policy: MissingArgumentPolicy::Reject,
    })
    .expect_err("missing optional argument must fail under strict policy");

    assert!(error.to_string().contains("does not have argument"));
}

#[test]
fn rejects_common_lisp_key_parameter_when_call_keyword_is_missing_under_strict_policy() {
    let input = "(defun f (a &key b c) (list a b c))\n(print (f 1 :c 30))";
    let error = plan_remove_function_parameter(RemoveFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("b"),
        call_paths: vec![path("1.1")],
        all_calls: false,
        missing_argument_policy: MissingArgumentPolicy::Reject,
    })
    .expect_err("missing keyword argument must fail under strict policy");

    assert!(
        error
            .to_string()
            .contains("does not have keyword argument :b")
    );
}

#[test]
fn rejects_common_lisp_key_parameter_named_as_keyword() {
    let input = "(defun f (a &key :b) (list a))\n(print (f 1 :b 20))";
    let error = plan_remove_function_parameter(RemoveFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol(":b"),
        call_paths: vec![path("1.1")],
        all_calls: false,
        missing_argument_policy: MissingArgumentPolicy::Reject,
    })
    .expect_err("keyword-named parameter must fail");

    assert!(
        error
            .to_string()
            .contains("currently supports only simple parameters")
    );
}

#[test]
fn rejects_common_lisp_key_parameter_with_non_keyword_designator() {
    let input = "(defun f (a &key ((external b) 2)) (list a b))\n(print (f 1 :external 20))";
    let error = plan_remove_function_parameter(RemoveFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("b"),
        call_paths: vec![path("1.1")],
        all_calls: false,
        missing_argument_policy: MissingArgumentPolicy::Reject,
    })
    .expect_err("non-keyword external designator must fail");

    assert!(
        error
            .to_string()
            .contains("currently supports only simple parameters")
    );
}

#[test]
fn rejects_common_lisp_parameter_after_allow_other_keys_before_next_marker() {
    let input = "(defun f (a &key b &allow-other-keys c) (list a b c))\n(print (f 1 :b 20 :c 30))";
    let error = plan_remove_function_parameter(RemoveFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("c"),
        call_paths: vec![path("1.1")],
        all_calls: false,
        missing_argument_policy: MissingArgumentPolicy::Reject,
    })
    .expect_err("parameter after &allow-other-keys must fail");

    assert!(error.to_string().contains("after &allow-other-keys"));
}

#[test]
fn rejects_duplicate_common_lisp_keyword_argument_removal() {
    let input = "(defun f (a &key b) (list a b))\n(print (f 1 :b 20 :b 30))";
    let error = plan_remove_function_parameter(RemoveFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("b"),
        call_paths: vec![path("1.1")],
        all_calls: false,
        missing_argument_policy: MissingArgumentPolicy::Reject,
    })
    .expect_err("duplicate keyword must fail");

    assert!(error.to_string().contains("duplicate keyword argument :b"));
}
