use super::*;

#[test]
fn moves_parameter_and_call_argument() {
    let input = "(defun f (a b c) (list a b c))\n(print (f 1 2 3))";
    let plan = plan_move_function_parameter(MoveFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("c"),
        to_index: 0,
        call_paths: vec![path("1.1")],
        all_calls: false,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defun f (c a b) (list a b c))\n(print (f 3 1 2))"
    );
    assert_eq!(plan.moved_arguments, vec!["3"]);
}

#[test]
fn rejects_move_when_the_parameter_list_contains_a_comment() {
    let input = "(defun f (a b\n          ;; c is optional context\n          c) (list a b c))\n(print (f 1 2 3))";
    let error = plan_move_function_parameter(MoveFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("c"),
        to_index: 0,
        call_paths: vec![path("1.1")],
        all_calls: false,
    })
    .expect_err("a comment in the parameter list must not be silently discarded");

    assert!(error.to_string().contains("comment"));
}

#[test]
fn moves_parameter_within_common_lisp_macrolet_binding_and_call_argument() {
    let input = "(defun outer ()\n  (macrolet ((render (width height margin) `(list ,width ,height ,margin)))\n    (render 10 20 5)))";
    let plan = plan_move_function_parameter(MoveFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0.3.1.0"),
        name: symbol("margin"),
        to_index: 0,
        call_paths: vec![path("0.3.2")],
        all_calls: false,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defun outer ()\n  (macrolet ((render (margin width height) `(list ,width ,height ,margin)))\n    (render 5 10 20)))"
    );
    assert_eq!(plan.moved_arguments, vec!["5"]);
}

#[test]
fn moves_parameter_within_common_lisp_cl_user_macrolet_binding_and_call_argument() {
    let input = "(defun outer ()\n  (cl-user:macrolet ((render (width height margin) `(list ,width ,height ,margin)))\n    (render 10 20 5)))";
    let plan = plan_move_function_parameter(MoveFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0.3.1.0"),
        name: symbol("margin"),
        to_index: 0,
        call_paths: vec![path("0.3.2")],
        all_calls: false,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defun outer ()\n  (cl-user:macrolet ((render (margin width height) `(list ,width ,height ,margin)))\n    (render 5 10 20)))"
    );
    assert_eq!(plan.moved_arguments, vec!["5"]);
}

#[test]
fn moves_parameter_within_common_lisp_labels_binding_and_call_argument() {
    let input = "(defun outer ()\n  (labels ((render (width height margin) (list width height margin)))\n    (render 10 20 5)))";
    let plan = plan_move_function_parameter(MoveFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0.3.1.0"),
        name: symbol("margin"),
        to_index: 0,
        call_paths: vec![path("0.3.2")],
        all_calls: false,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defun outer ()\n  (labels ((render (margin width height) (list width height margin)))\n    (render 5 10 20)))"
    );
    assert_eq!(plan.moved_arguments, vec!["5"]);
}

#[test]
fn moves_parameter_within_common_lisp_compiler_macrolet_binding_and_call_argument() {
    let input = "(defun outer ()\n  (compiler-macrolet ((render (width height margin) `(list ,width ,height ,margin)))\n    (render 10 20 5)))";
    let plan = plan_move_function_parameter(MoveFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0.3.1.0"),
        name: symbol("margin"),
        to_index: 0,
        call_paths: vec![path("0.3.2")],
        all_calls: false,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defun outer ()\n  (compiler-macrolet ((render (margin width height) `(list ,width ,height ,margin)))\n    (render 5 10 20)))"
    );
    assert_eq!(plan.moved_arguments, vec!["5"]);
}

#[test]
fn moves_parameter_within_common_lisp_cl_user_compiler_macrolet_binding_and_call_argument() {
    let input = "(defun outer ()\n  (cl-user:compiler-macrolet ((render (width height margin) `(list ,width ,height ,margin)))\n    (render 10 20 5)))";
    let plan = plan_move_function_parameter(MoveFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0.3.1.0"),
        name: symbol("margin"),
        to_index: 0,
        call_paths: vec![path("0.3.2")],
        all_calls: false,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defun outer ()\n  (cl-user:compiler-macrolet ((render (margin width height) `(list ,width ,height ,margin)))\n    (render 5 10 20)))"
    );
    assert_eq!(plan.moved_arguments, vec!["5"]);
}

#[test]
fn moves_parameter_within_common_lisp_define_setf_expander_call_reports_rewritten_call() {
    let input = "\
(define-setf-expander access (object mode extra)
  (values nil nil nil `(setf-helper ,object)))
(setf (access item :rw :fast) 1)";
    let plan = plan_move_function_parameter(MoveFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("extra"),
        to_index: 1,
        call_paths: vec![path("1")],
        all_calls: false,
    })
    .expect("plan");

    assert_eq!(plan.call_paths, vec![path("1")]);
    assert_eq!(
        plan.rewritten,
        "\
(define-setf-expander access (object extra mode)
  (values nil nil nil `(setf-helper ,object)))
(setf (access item :fast :rw) 1)"
    );
    assert_eq!(plan.moved_arguments, vec![":fast"]);
}

#[test]
fn moves_parameter_within_common_lisp_long_form_defsetf_call() {
    let input = "\
(defsetf access (object mode extra) (value)
  `(update-access ,object ,value))
(setf (access item :rw :fast) 1)";
    let plan = plan_move_function_parameter(MoveFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("extra"),
        to_index: 1,
        call_paths: vec![path("1")],
        all_calls: false,
    })
    .expect("plan");

    assert_eq!(plan.call_paths, vec![path("1")]);
    assert_eq!(
        plan.rewritten,
        "\
(defsetf access (object extra mode) (value)
  `(update-access ,object ,value))
(setf (access item :fast :rw) 1)"
    );
    assert_eq!(plan.moved_arguments, vec![":fast"]);
}

#[test]
fn moves_parameter_within_common_lisp_define_modify_macro_call() {
    let input = "\
(define-modify-macro appendf (item tail extra) append)
(appendf place 10 20 30)";
    let plan = plan_move_function_parameter(MoveFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("extra"),
        to_index: 1,
        call_paths: vec![path("1")],
        all_calls: false,
    })
    .expect("plan");

    assert_eq!(plan.call_paths, vec![path("1")]);
    assert_eq!(
        plan.rewritten,
        "\
(define-modify-macro appendf (item extra tail) append)
(appendf place 10 30 20)"
    );
    assert_eq!(plan.moved_arguments, vec!["30"]);
}

#[test]
fn moves_parameter_within_common_lisp_define_setf_expander_call() {
    let input = "\
(define-setf-expander access (object mode extra)
  (values nil nil nil `(setf-helper ,object)))
(setf (access item :rw :fast) 1)";
    let plan = plan_move_function_parameter(MoveFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("extra"),
        to_index: 1,
        call_paths: vec![path("1")],
        all_calls: false,
    })
    .expect("plan");

    assert_eq!(plan.call_paths, vec![path("1")]);
    assert_eq!(
        plan.rewritten,
        "\
(define-setf-expander access (object extra mode)
  (values nil nil nil `(setf-helper ,object)))
(setf (access item :fast :rw) 1)"
    );
    assert_eq!(plan.moved_arguments, vec![":fast"]);
}

#[test]
fn moves_common_lisp_optional_parameter_and_call_argument_within_section() {
    let input = "(defun f (a &optional b c) (list a b c))\n(print (f 1 2 3))";
    let plan = plan_move_function_parameter(MoveFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("c"),
        to_index: 1,
        call_paths: vec![path("1.1")],
        all_calls: false,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defun f (a &optional c b) (list a b c))\n(print (f 1 3 2))"
    );
    assert_eq!(plan.moved_arguments, vec!["3"]);
}

#[test]
fn moves_common_lisp_key_parameter_and_call_argument_within_section() {
    let input = "(defun f (a &key b c) (list a b c))\n(print (f 1 :b 2 :c 3))";
    let plan = plan_move_function_parameter(MoveFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("c"),
        to_index: 1,
        call_paths: vec![path("1.1")],
        all_calls: false,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defun f (a &key c b) (list a b c))\n(print (f 1 :c 3 :b 2))"
    );
    assert_eq!(plan.moved_arguments, vec!["3"]);
}

#[test]
fn rejects_move_parameter_across_common_lisp_lambda_list_section() {
    let input = "(defun f (a &optional b) (list a b))\n(print (f 1 2))";
    let error = plan_move_function_parameter(MoveFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("b"),
        to_index: 0,
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
        error.to_string().starts_with("move-function-parameter"),
        "move error must name its own command, got: {error}"
    );
}

#[test]
fn move_parameter_call_arity_error_names_its_own_command() {
    let input = "(defun f (a b c) (list a b c))\n(print (f 1 2))";
    let error = plan_move_function_parameter(MoveFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("c"),
        to_index: 0,
        call_paths: vec![path("1.1")],
        all_calls: false,
    })
    .expect_err("call with too few positional arguments must fail");

    assert!(
        error.to_string().starts_with("move-function-parameter"),
        "call-arity error must name its own command, got: {error}"
    );
}
