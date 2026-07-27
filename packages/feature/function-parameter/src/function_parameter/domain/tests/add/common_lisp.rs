use super::*;

#[test]
fn adds_common_lisp_optional_parameter_at_start_before_key_section() {
    let input = "(defun render (node &optional stream &key color) (list node stream style color))\n(render item out :color :red)";
    let plan = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("style"),
        argument: ":compact".to_owned(),
        call_paths: vec![path("1")],
        all_calls: false,
        insert: FunctionParameterInsert::Start,
        section: FunctionParameterSection::Optional,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defun render (node &optional style stream &key color) (list node stream style color))\n(render item :compact out :color :red)"
    );
}

#[test]
fn adds_common_lisp_required_parameter_before_rest_marker() {
    let input = "(defun collect (head &rest tail) (list head value tail))\n(collect 1 2 3)";
    let plan = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("value"),
        argument: "10".to_owned(),
        call_paths: vec![path("1")],
        all_calls: false,
        insert: FunctionParameterInsert::End,
        section: FunctionParameterSection::Auto,
    })
    .expect("plan");

    assert_eq!(plan.section, FunctionParameterSection::Positional);
    assert_eq!(
        plan.rewritten,
        "(defun collect (head value &rest tail) (list head value tail))\n(collect 1 10 2 3)"
    );
}

#[test]
fn adds_common_lisp_required_parameter_before_dotted_tail() {
    let input = "(defun collect (head . tail) (list head tail))\n(collect 1 2 3)";
    let plan = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("value"),
        argument: "10".to_owned(),
        call_paths: vec![path("1")],
        all_calls: false,
        insert: FunctionParameterInsert::End,
        section: FunctionParameterSection::Auto,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defun collect (head value . tail) (list head tail))\n(collect 1 10 2 3)"
    );
}

#[test]
fn adds_common_lisp_required_parameter_before_body_marker_in_macro() {
    let input = "(defmacro collect-body (head &body body) `(list ,head value ,@body))\n(collect-body 1 (+ 2 3) (+ 4 5))";
    let plan = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("value"),
        argument: "10".to_owned(),
        call_paths: vec![path("1")],
        all_calls: false,
        insert: FunctionParameterInsert::End,
        section: FunctionParameterSection::Auto,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defmacro collect-body (head value &body body) `(list ,head value ,@body))\n(collect-body 1 10 (+ 2 3) (+ 4 5))"
    );
}

#[test]
fn adds_common_lisp_required_parameter_before_optional_and_key_sections_when_requested() {
    let input = "(defun render (node &optional stream &key color) (list node stream color req))\n(render item out :color :red)";
    let plan = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("req"),
        argument: "42".to_owned(),
        call_paths: vec![path("1")],
        all_calls: false,
        insert: FunctionParameterInsert::End,
        section: FunctionParameterSection::Positional,
    })
    .expect("plan");

    assert_eq!(plan.section, FunctionParameterSection::Positional);
    assert_eq!(
        plan.rewritten,
        "(defun render (node req &optional stream &key color) (list node stream color req))\n(render item 42 out :color :red)"
    );
}

#[test]
fn adds_common_lisp_required_parameter_before_optional_section_when_requested() {
    let input = "(defun render (node &optional stream) (list req node stream))\n(render item out)";
    let plan = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("req"),
        argument: "42".to_owned(),
        call_paths: vec![path("1")],
        all_calls: false,
        insert: FunctionParameterInsert::End,
        section: FunctionParameterSection::Positional,
    })
    .expect("plan");

    assert_eq!(plan.section, FunctionParameterSection::Positional);
    assert_eq!(
        plan.rewritten,
        "(defun render (node req &optional stream) (list req node stream))\n(render item 42 out)"
    );
}

#[test]
fn adds_parameter_to_emacs_lisp_cl_defgeneric() {
    let input = "(cl-defgeneric render (node stream context))\n(render thing out :fancy)";
    let plan = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::EmacsLisp,
        definition_path: path("0"),
        name: symbol("style"),
        argument: ":compact".to_owned(),
        call_paths: vec![path("1")],
        all_calls: false,
        insert: FunctionParameterInsert::End,
        section: FunctionParameterSection::Auto,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(cl-defgeneric render (node stream context style))\n(render thing out :fancy :compact)"
    );
}

#[test]
fn adds_parameter_to_common_lisp_define_compiler_macro() {
    let input = "(define-compiler-macro collect-body (head &body body) `(list ,head value ,@body))\n(collect-body 1 (+ 2 3) (+ 4 5))";
    let plan = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("value"),
        argument: "10".to_owned(),
        call_paths: vec![path("1")],
        all_calls: false,
        insert: FunctionParameterInsert::End,
        section: FunctionParameterSection::Auto,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(define-compiler-macro collect-body (head value &body body) `(list ,head value ,@body))\n(collect-body 1 10 (+ 2 3) (+ 4 5))"
    );
}

#[test]
fn adds_parameter_to_common_lisp_macrolet_binding() {
    let input = "(defun outer ()\n  (macrolet ((with-log (message) `(list ,message)))\n    (with-log \"hello\")))";
    let plan = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0.3.1.0"),
        name: symbol("context"),
        argument: ":ctx".to_owned(),
        call_paths: vec![path("0.3.2")],
        all_calls: false,
        insert: FunctionParameterInsert::End,
        section: FunctionParameterSection::Auto,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defun outer ()\n  (macrolet ((with-log (message context) `(list ,message)))\n    (with-log \"hello\" :ctx)))"
    );
}

#[test]
fn adds_parameter_to_common_lisp_cl_user_macrolet_binding() {
    let input = "(defun outer ()\n  (cl-user:macrolet ((with-log (message) `(list ,message)))\n    (with-log \"hello\")))";
    let plan = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0.3.1.0"),
        name: symbol("context"),
        argument: ":ctx".to_owned(),
        call_paths: vec![path("0.3.2")],
        all_calls: false,
        insert: FunctionParameterInsert::End,
        section: FunctionParameterSection::Auto,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defun outer ()\n  (cl-user:macrolet ((with-log (message context) `(list ,message)))\n    (with-log \"hello\" :ctx)))"
    );
}

#[test]
fn adds_parameter_to_common_lisp_compiler_macrolet_binding() {
    let input = "(defun outer ()\n  (compiler-macrolet ((expand-it (form env) `(list ,form ,env)))\n    (expand-it input env)))";
    let plan = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0.3.1.0"),
        name: symbol("context"),
        argument: ":ctx".to_owned(),
        call_paths: vec![path("0.3.2")],
        all_calls: false,
        insert: FunctionParameterInsert::End,
        section: FunctionParameterSection::Auto,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defun outer ()\n  (compiler-macrolet ((expand-it (form env context) `(list ,form ,env)))\n    (expand-it input env :ctx)))"
    );
}

#[test]
fn adds_parameter_to_common_lisp_cl_user_compiler_macrolet_binding() {
    let input = "(defun outer ()\n  (cl-user:compiler-macrolet ((expand-it (form env) `(list ,form ,env)))\n    (expand-it input env)))";
    let plan = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0.3.1.0"),
        name: symbol("context"),
        argument: ":ctx".to_owned(),
        call_paths: vec![path("0.3.2")],
        all_calls: false,
        insert: FunctionParameterInsert::End,
        section: FunctionParameterSection::Auto,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defun outer ()\n  (cl-user:compiler-macrolet ((expand-it (form env context) `(list ,form ,env)))\n    (expand-it input env :ctx)))"
    );
}

#[test]
fn adds_parameter_to_common_lisp_define_setf_expander_call() {
    let input = "\
(define-setf-expander access (object)
  (values nil nil nil `(setf-helper ,object)))
(setf (access item) 1)";
    let plan = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("mode"),
        argument: ":rw".to_owned(),
        call_paths: vec![path("1")],
        all_calls: false,
        insert: FunctionParameterInsert::End,
        section: FunctionParameterSection::Auto,
    })
    .expect("plan");

    assert_eq!(plan.call_paths, vec![path("1")]);
    assert_eq!(
        plan.rewritten,
        "\
(define-setf-expander access (object mode)
  (values nil nil nil `(setf-helper ,object)))
(setf (access item :rw) 1)"
    );
}

#[test]
fn adds_parameter_to_common_lisp_long_form_defsetf_call() {
    let input = "\
(defsetf access (object) (value)
  `(update-access ,object ,value))
(setf (access item) 1)";
    let plan = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("mode"),
        argument: ":rw".to_owned(),
        call_paths: vec![path("1")],
        all_calls: false,
        insert: FunctionParameterInsert::End,
        section: FunctionParameterSection::Auto,
    })
    .expect("plan");

    assert_eq!(plan.call_paths, vec![path("1")]);
    assert_eq!(
        plan.rewritten,
        "\
(defsetf access (object mode) (value)
  `(update-access ,object ,value))
(setf (access item :rw) 1)"
    );
}

#[test]
fn adds_define_modify_macro_parameter_after_implicit_place_argument() {
    let input = "(define-modify-macro appendf (item) append)\n(appendf place 10)";
    let plan = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("tail"),
        argument: "20".to_owned(),
        call_paths: vec![path("1")],
        all_calls: false,
        insert: FunctionParameterInsert::End,
        section: FunctionParameterSection::Auto,
    })
    .expect("define-modify-macro should support function-parameter refactor");

    assert_eq!(
        plan.rewritten,
        "(define-modify-macro appendf (item tail) append)\n(appendf place 10 20)"
    );
}
