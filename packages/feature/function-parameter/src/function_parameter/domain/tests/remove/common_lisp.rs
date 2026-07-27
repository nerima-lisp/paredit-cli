use super::*;

#[test]
fn removes_parameter_from_common_lisp_flet_binding_and_call_argument() {
    let input = "(defun outer ()\n  (flet ((render (width height margin) (list width height margin)))\n    (render 10 20 5)))";
    let plan = plan_remove_function_parameter(RemoveFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0.3.1.0"),
        name: symbol("margin"),
        call_paths: vec![path("0.3.2")],
        all_calls: false,
        missing_argument_policy: MissingArgumentPolicy::Reject,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defun outer ()\n  (flet ((render (width height) (list width height margin)))\n    (render 10 20)))"
    );
    assert_eq!(plan.removed_arguments, vec![Some("5".to_owned())]);
}

#[test]
fn removes_parameter_from_common_lisp_macrolet_binding_and_call_argument() {
    let input = "(defun outer ()\n  (macrolet ((render (width height margin) `(list ,width ,height ,margin)))\n    (render 10 20 5)))";
    let plan = plan_remove_function_parameter(RemoveFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0.3.1.0"),
        name: symbol("margin"),
        call_paths: vec![path("0.3.2")],
        all_calls: false,
        missing_argument_policy: MissingArgumentPolicy::Reject,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defun outer ()\n  (macrolet ((render (width height) `(list ,width ,height ,margin)))\n    (render 10 20)))"
    );
    assert_eq!(plan.removed_arguments, vec![Some("5".to_owned())]);
}

#[test]
fn removes_parameter_from_common_lisp_cl_user_macrolet_binding_and_call_argument() {
    let input = "(defun outer ()\n  (cl-user:macrolet ((render (width height margin) `(list ,width ,height ,margin)))\n    (render 10 20 5)))";
    let plan = plan_remove_function_parameter(RemoveFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0.3.1.0"),
        name: symbol("margin"),
        call_paths: vec![path("0.3.2")],
        all_calls: false,
        missing_argument_policy: MissingArgumentPolicy::Reject,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defun outer ()\n  (cl-user:macrolet ((render (width height) `(list ,width ,height ,margin)))\n    (render 10 20)))"
    );
    assert_eq!(plan.removed_arguments, vec![Some("5".to_owned())]);
}

#[test]
fn removes_parameter_from_common_lisp_compiler_macrolet_binding_and_call_argument() {
    let input = "(defun outer ()\n  (compiler-macrolet ((render (width height margin) `(list ,width ,height ,margin)))\n    (render 10 20 5)))";
    let plan = plan_remove_function_parameter(RemoveFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0.3.1.0"),
        name: symbol("margin"),
        call_paths: vec![path("0.3.2")],
        all_calls: false,
        missing_argument_policy: MissingArgumentPolicy::Reject,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defun outer ()\n  (compiler-macrolet ((render (width height) `(list ,width ,height ,margin)))\n    (render 10 20)))"
    );
    assert_eq!(plan.removed_arguments, vec![Some("5".to_owned())]);
}

#[test]
fn removes_parameter_from_common_lisp_cl_user_compiler_macrolet_binding_and_call_argument() {
    let input = "(defun outer ()\n  (cl-user:compiler-macrolet ((render (width height margin) `(list ,width ,height ,margin)))\n    (render 10 20 5)))";
    let plan = plan_remove_function_parameter(RemoveFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0.3.1.0"),
        name: symbol("margin"),
        call_paths: vec![path("0.3.2")],
        all_calls: false,
        missing_argument_policy: MissingArgumentPolicy::Reject,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defun outer ()\n  (cl-user:compiler-macrolet ((render (width height) `(list ,width ,height ,margin)))\n    (render 10 20)))"
    );
    assert_eq!(plan.removed_arguments, vec![Some("5".to_owned())]);
}

#[test]
fn removes_parameter_from_common_lisp_define_setf_expander_call_with_declare_body() {
    let input = "\
(define-setf-expander access (object mode)
  (values nil nil nil `(setf-helper ,object)))
(setf (access item :rw) 1)";
    let plan = plan_remove_function_parameter(RemoveFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("mode"),
        call_paths: vec![path("1")],
        all_calls: false,
        missing_argument_policy: MissingArgumentPolicy::Reject,
    })
    .expect("plan");

    assert_eq!(plan.call_paths, vec![path("1")]);
    assert_eq!(
        plan.rewritten,
        "\
(define-setf-expander access (object)
  (values nil nil nil `(setf-helper ,object)))
(setf (access item) 1)"
    );
    assert_eq!(plan.removed_arguments, vec![Some(":rw".to_owned())]);
}

#[test]
fn removes_parameter_from_common_lisp_long_form_defsetf_call() {
    let input = "\
(defsetf access (object mode) (value)
  `(update-access ,object ,value))
(setf (access item :rw) 1)";
    let plan = plan_remove_function_parameter(RemoveFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("mode"),
        call_paths: vec![path("1")],
        all_calls: false,
        missing_argument_policy: MissingArgumentPolicy::Reject,
    })
    .expect("plan");

    assert_eq!(plan.call_paths, vec![path("1")]);
    assert_eq!(
        plan.rewritten,
        "\
(defsetf access (object) (value)
  `(update-access ,object ,value))
(setf (access item) 1)"
    );
    assert_eq!(plan.removed_arguments, vec![Some(":rw".to_owned())]);
}

#[test]
fn removes_parameter_from_common_lisp_define_modify_macro_call() {
    let input = "\
(define-modify-macro appendf (item tail) append)
(appendf place 10 20)";
    let plan = plan_remove_function_parameter(RemoveFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("tail"),
        call_paths: vec![path("1")],
        all_calls: false,
        missing_argument_policy: MissingArgumentPolicy::Reject,
    })
    .expect("plan");

    assert_eq!(plan.call_paths, vec![path("1")]);
    assert_eq!(
        plan.rewritten,
        "\
(define-modify-macro appendf (item) append)
(appendf place 10)"
    );
    assert_eq!(plan.removed_arguments, vec![Some("20".to_owned())]);
}

#[test]
fn removes_parameter_from_common_lisp_define_setf_expander_call() {
    let input = "\
(define-setf-expander access (object slot)
  (declare (ignore object slot))
  (values nil nil nil nil nil))
(setf (access item :mode) 1)";
    let plan = plan_remove_function_parameter(RemoveFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("slot"),
        call_paths: vec![path("1")],
        all_calls: false,
        missing_argument_policy: MissingArgumentPolicy::Reject,
    })
    .expect("plan");

    assert_eq!(plan.call_paths, vec![path("1")]);
    assert_eq!(
        plan.rewritten,
        "\
(define-setf-expander access (object)
  (declare (ignore object slot))
  (values nil nil nil nil nil))
(setf (access item) 1)"
    );
    assert_eq!(plan.removed_arguments, vec![Some(":mode".to_owned())]);
}

#[test]
fn removes_parameter_from_cl_qualified_define_setf_expander_call() {
    let input = "\
(cl:define-setf-expander access (object slot)
  (declare (ignore object slot))
  (values nil nil nil nil nil))
(setf (access item :mode) 1)";
    let plan = plan_remove_function_parameter(RemoveFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("slot"),
        call_paths: vec![path("1")],
        all_calls: false,
        missing_argument_policy: MissingArgumentPolicy::Reject,
    })
    .expect("plan");

    assert_eq!(plan.call_paths, vec![path("1")]);
    assert_eq!(
        plan.rewritten,
        "\
(cl:define-setf-expander access (object)
  (declare (ignore object slot))
  (values nil nil nil nil nil))
(setf (access item) 1)"
    );
    assert_eq!(plan.removed_arguments, vec![Some(":mode".to_owned())]);
}

#[test]
fn adds_parameter_to_common_lisp_defmethod_and_call() {
    let input =
        "(defmethod render :around ((node widget) stream) (draw node stream))\n(render thing out)";
    let plan = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("style"),
        argument: ":fancy".to_owned(),
        call_paths: vec![path("1")],
        all_calls: false,
        insert: FunctionParameterInsert::End,
        section: FunctionParameterSection::Auto,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defmethod render :around ((node widget) stream style) (draw node stream))\n(render thing out :fancy)"
    );
}

#[test]
fn removes_specialized_parameter_from_common_lisp_defmethod_and_call() {
    let input = "(defmethod render :around ((node widget) stream style) (draw stream style))\n(render thing out :fancy)";
    let plan = plan_remove_function_parameter(RemoveFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("node"),
        call_paths: vec![path("1")],
        all_calls: false,
        missing_argument_policy: MissingArgumentPolicy::Reject,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defmethod render :around (stream style) (draw stream style))\n(render out :fancy)"
    );
    assert_eq!(plan.parameter_index, 0);
    assert_eq!(plan.removed_arguments, vec![Some("thing".to_owned())]);
}

#[test]
fn removes_common_lisp_optional_parameter_spec_and_call_argument() {
    let input = "(defun f (a &optional (b 2 b-p) c) (list a b c))\n(print (f 1 3 4))";
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

    assert_eq!(
        plan.rewritten,
        "(defun f (a &optional c) (list a b c))\n(print (f 1 4))"
    );
    assert_eq!(plan.parameter_index, 1);
    assert_eq!(plan.removed_arguments, vec![Some("3".to_owned())]);
}

#[test]
fn removes_common_lisp_optional_parameter_when_call_argument_is_missing() {
    let input = "(defun f (a &optional (b 2 b-p) c) (list a b c))\n(print (f 1))";
    let plan = plan_remove_function_parameter(RemoveFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("b"),
        call_paths: vec![path("1.1")],
        all_calls: false,
        missing_argument_policy: MissingArgumentPolicy::Ignore,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defun f (a &optional c) (list a b c))\n(print (f 1))"
    );
    assert_eq!(plan.parameter_index, 1);
    assert_eq!(plan.removed_arguments, vec![None]);
}

#[test]
fn removes_common_lisp_key_parameter_and_call_keyword_argument() {
    let input = "(defun f (a &key (b 2) ((:external c) 3 c-p) d) (list a b c d))\n(print (f 1 :b 20 :external 30 :d 40))";
    let plan = plan_remove_function_parameter(RemoveFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("c"),
        call_paths: vec![path("1.1")],
        all_calls: false,
        missing_argument_policy: MissingArgumentPolicy::Reject,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defun f (a &key (b 2) d) (list a b c d))\n(print (f 1 :b 20 :d 40))"
    );
    assert_eq!(plan.parameter_keyword.as_deref(), Some(":external"));
    assert_eq!(
        plan.removed_arguments,
        vec![Some(":external 30".to_owned())]
    );
}

#[test]
fn removes_common_lisp_key_parameter_when_call_keyword_is_missing() {
    let input = "(defun f (a &key b c) (list a b c))\n(print (f 1 :c 30))";
    let plan = plan_remove_function_parameter(RemoveFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("b"),
        call_paths: vec![path("1.1")],
        all_calls: false,
        missing_argument_policy: MissingArgumentPolicy::Ignore,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defun f (a &key c) (list a b c))\n(print (f 1 :c 30))"
    );
    assert_eq!(plan.parameter_keyword.as_deref(), Some(":b"));
    assert_eq!(plan.removed_arguments, vec![None]);
}

#[test]
fn removes_common_lisp_dotted_tail_parameter_without_touching_calls() {
    let input = "(defun collect (head . tail) (list head tail))\n(collect 1 2 3)";
    let plan = plan_remove_function_parameter(RemoveFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("tail"),
        call_paths: vec![path("1")],
        all_calls: false,
        missing_argument_policy: MissingArgumentPolicy::Reject,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defun collect (head) (list head tail))\n(collect 1 2 3)"
    );
    assert_eq!(plan.removed_arguments, vec![None]);
}

#[test]
fn adds_parameter_before_common_lisp_rest_marker() {
    let input = "(defun f (a &rest b) (list a b))\n(print (f 1 2))";
    let plan = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("c"),
        argument: "3".to_owned(),
        call_paths: vec![path("1.1")],
        all_calls: false,
        insert: FunctionParameterInsert::End,
        section: FunctionParameterSection::Auto,
    })
    .expect("rest insertion should succeed");

    assert_eq!(
        plan.rewritten,
        "(defun f (a c &rest b) (list a b))\n(print (f 1 3 2))"
    );
}
