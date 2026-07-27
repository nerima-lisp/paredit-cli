use super::*;

#[test]
fn discovers_all_same_file_calls() {
    let input = "(defun f (a) a)\n(print (f 1))\n(print (f 2))";
    let plan = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("b"),
        argument: "0".to_owned(),
        call_paths: Vec::new(),
        all_calls: true,
        insert: FunctionParameterInsert::End,
        section: FunctionParameterSection::Auto,
    })
    .expect("plan");

    assert_eq!(plan.call_paths, vec![path("1.1"), path("2.1")]);
    assert_eq!(
        plan.rewritten,
        "(defun f (a b) a)\n(print (f 1 0))\n(print (f 2 0))"
    );
}

#[test]
fn discovers_all_calls_respects_common_lisp_flet_callable_shadowing() {
    let input = "\
(defun f (a) a)
(defun caller ()
  (flet ((f (x) (f x)))
    (f 1))
  (f 2))";
    let plan = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("b"),
        argument: "0".to_owned(),
        call_paths: Vec::new(),
        all_calls: true,
        insert: FunctionParameterInsert::End,
        section: FunctionParameterSection::Auto,
    })
    .expect("plan");

    assert_eq!(plan.call_paths, vec![path("1.3.1.0.2"), path("1.4")]);
    assert_eq!(
        plan.rewritten,
        "\
(defun f (a b) a)
(defun caller ()
  (flet ((f (x) (f x 0)))
    (f 1))
  (f 2 0))"
    );
}

#[test]
fn discovers_all_calls_respects_common_lisp_cl_user_flet_callable_shadowing() {
    let input = "\
(defun f (a) a)
(defun caller ()
  (cl-user:flet ((f (x) (f x)))
    (f 1))
  (f 2))";
    let plan = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("b"),
        argument: "0".to_owned(),
        call_paths: Vec::new(),
        all_calls: true,
        insert: FunctionParameterInsert::End,
        section: FunctionParameterSection::Auto,
    })
    .expect("plan");

    assert_eq!(plan.call_paths, vec![path("1.3.1.0.2"), path("1.4")]);
    assert_eq!(
        plan.rewritten,
        "\
(defun f (a b) a)
(defun caller ()
  (cl-user:flet ((f (x) (f x 0)))
    (f 1))
  (f 2 0))"
    );
}

#[test]
fn discovers_all_calls_respects_common_lisp_labels_callable_shadowing() {
    let input = "\
(defun f (a) a)
(defun caller ()
  (labels ((f (x) (f x))
           (g (y) (f y)))
    (f 1)
    (cl:print (f 2)))
  (f 3))";
    let plan = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("b"),
        argument: "0".to_owned(),
        call_paths: Vec::new(),
        all_calls: true,
        insert: FunctionParameterInsert::End,
        section: FunctionParameterSection::Auto,
    })
    .expect("plan");

    assert_eq!(plan.call_paths, vec![path("1.4")]);
    assert_eq!(
        plan.rewritten,
        "\
(defun f (a b) a)
(defun caller ()
  (labels ((f (x) (f x))
           (g (y) (f y)))
    (f 1)
    (cl:print (f 2)))
  (f 3 0))"
    );
}

#[test]
fn discovers_all_calls_respects_common_lisp_cl_user_labels_callable_shadowing() {
    let input = "\
(defun f (a) a)
(defun caller ()
  (cl-user:labels ((f (x) (f x))
                   (g (y) (f y)))
    (f 1)
    (cl:print (f 2)))
  (f 3))";
    let plan = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("b"),
        argument: "0".to_owned(),
        call_paths: Vec::new(),
        all_calls: true,
        insert: FunctionParameterInsert::End,
        section: FunctionParameterSection::Auto,
    })
    .expect("plan");

    assert_eq!(plan.call_paths, vec![path("1.4")]);
    assert_eq!(
        plan.rewritten,
        "\
(defun f (a b) a)
(defun caller ()
  (cl-user:labels ((f (x) (f x))
                   (g (y) (f y)))
    (f 1)
    (cl:print (f 2)))
  (f 3 0))"
    );
}

#[test]
fn discovers_all_calls_respects_common_lisp_macrolet_callable_shadowing() {
    let input = "\
(defun f (a) a)
(defun caller ()
  (macrolet ((f (x) (f x)))
    (f 1))
  (f 2))";
    let plan = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("b"),
        argument: "0".to_owned(),
        call_paths: Vec::new(),
        all_calls: true,
        insert: FunctionParameterInsert::End,
        section: FunctionParameterSection::Auto,
    })
    .expect("plan");

    assert_eq!(plan.call_paths, vec![path("1.3.1.0.2"), path("1.4")]);
    assert_eq!(
        plan.rewritten,
        "\
(defun f (a b) a)
(defun caller ()
  (macrolet ((f (x) (f x 0)))
    (f 1))
  (f 2 0))"
    );
}

#[test]
fn discovers_all_calls_respects_common_lisp_cl_user_macrolet_callable_shadowing() {
    let input = "\
(defun f (a) a)
(defun caller ()
  (cl-user:macrolet ((f (x) (f x)))
    (f 1))
  (f 2))";
    let plan = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("b"),
        argument: "0".to_owned(),
        call_paths: Vec::new(),
        all_calls: true,
        insert: FunctionParameterInsert::End,
        section: FunctionParameterSection::Auto,
    })
    .expect("plan");

    assert_eq!(plan.call_paths, vec![path("1.3.1.0.2"), path("1.4")]);
    assert_eq!(
        plan.rewritten,
        "\
(defun f (a b) a)
(defun caller ()
  (cl-user:macrolet ((f (x) (f x 0)))
    (f 1))
  (f 2 0))"
    );
}

#[test]
fn discovers_all_calls_respects_common_lisp_compiler_macrolet_callable_shadowing() {
    let input = "\
(defun f (a) a)
(defun caller ()
  (compiler-macrolet ((f (x) (f x)))
    (f 1))
  (f 2))";
    let plan = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("b"),
        argument: "0".to_owned(),
        call_paths: Vec::new(),
        all_calls: true,
        insert: FunctionParameterInsert::End,
        section: FunctionParameterSection::Auto,
    })
    .expect("plan");

    assert_eq!(plan.call_paths, vec![path("1.3.1.0.2"), path("1.4")]);
    assert_eq!(
        plan.rewritten,
        "\
(defun f (a b) a)
(defun caller ()
  (compiler-macrolet ((f (x) (f x 0)))
    (f 1))
  (f 2 0))"
    );
}

#[test]
fn discovers_common_lisp_setf_place_calls_for_define_setf_expander() {
    let input = "\
(define-setf-expander access (object)
  (values nil nil nil `(setf-helper ,object)))
(setf (access item) 1)
(print (access other))";
    let plan = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("mode"),
        argument: ":rw".to_owned(),
        call_paths: Vec::new(),
        all_calls: true,
        insert: FunctionParameterInsert::End,
        section: FunctionParameterSection::Auto,
    })
    .expect("plan");

    assert_eq!(plan.call_paths, vec![path("1"), path("2.1")]);
    assert_eq!(
        plan.rewritten,
        "\
(define-setf-expander access (object mode)
  (values nil nil nil `(setf-helper ,object)))
(setf (access item :rw) 1)
(print (access other :rw))"
    );
}

#[test]
fn discovers_all_calls_respects_common_lisp_cl_user_compiler_macrolet_callable_shadowing() {
    let input = "\
(defun f (a) a)
(defun caller ()
  (cl-user:compiler-macrolet ((f (x) (f x)))
    (f 1))
  (f 2))";
    let plan = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("b"),
        argument: "0".to_owned(),
        call_paths: Vec::new(),
        all_calls: true,
        insert: FunctionParameterInsert::End,
        section: FunctionParameterSection::Auto,
    })
    .expect("plan");

    assert_eq!(plan.call_paths, vec![path("1.3.1.0.2"), path("1.4")]);
    assert_eq!(
        plan.rewritten,
        "\
(defun f (a b) a)
(defun caller ()
  (cl-user:compiler-macrolet ((f (x) (f x 0)))
    (f 1))
  (f 2 0))"
    );
}

#[test]
fn rejects_explicit_labels_shadowed_call_path() {
    let input = "\
(defun f (a) a)
(defun caller ()
  (labels ((f (x) (f x))
           (g (y) (f y)))
    (f 1)
    (cl:print (f 2)))
  (f 3))";
    let error = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("b"),
        argument: "0".to_owned(),
        call_paths: vec![path("1.3.1.0.2")],
        all_calls: false,
        insert: FunctionParameterInsert::End,
        section: FunctionParameterSection::Auto,
    })
    .expect_err("labels local call should be rejected");

    assert!(
        error
            .to_string()
            .contains("shadowed by a local callable binding or overlaps the selected definition")
    );
}

#[test]
fn rejects_explicit_cl_user_labels_shadowed_call_path() {
    let input = "\
(defun f (a) a)
(defun caller ()
  (cl-user:labels ((f (x) (f x))
                   (g (y) (f y)))
    (f 1)
    (cl:print (f 2)))
  (f 3))";
    let error = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("b"),
        argument: "0".to_owned(),
        call_paths: vec![path("1.3.1.0.2")],
        all_calls: false,
        insert: FunctionParameterInsert::End,
        section: FunctionParameterSection::Auto,
    })
    .expect_err("labels local call should be rejected");

    assert!(
        error
            .to_string()
            .contains("shadowed by a local callable binding or overlaps the selected definition")
    );
}

#[test]
fn rejects_explicit_macrolet_shadowed_call_path() {
    let input = "\
(defun f (a) a)
(defun caller ()
  (macrolet ((f (x) (f x)))
    (f 1))
  (f 2))";
    let error = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("b"),
        argument: "0".to_owned(),
        call_paths: vec![path("1.3.2")],
        all_calls: false,
        insert: FunctionParameterInsert::End,
        section: FunctionParameterSection::Auto,
    })
    .expect_err("macrolet body local call should be rejected");

    assert!(
        error
            .to_string()
            .contains("shadowed by a local callable binding or overlaps the selected definition")
    );
}

#[test]
fn rejects_explicit_cl_user_macrolet_shadowed_call_path() {
    let input = "\
(defun f (a) a)
(defun caller ()
  (cl-user:macrolet ((f (x) (f x)))
    (f 1))
  (f 2))";
    let error = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("b"),
        argument: "0".to_owned(),
        call_paths: vec![path("1.3.2")],
        all_calls: false,
        insert: FunctionParameterInsert::End,
        section: FunctionParameterSection::Auto,
    })
    .expect_err("macrolet body local call should be rejected");

    assert!(
        error
            .to_string()
            .contains("shadowed by a local callable binding or overlaps the selected definition")
    );
}

#[test]
fn rejects_explicit_compiler_macrolet_shadowed_call_path() {
    let input = "\
(defun f (a) a)
(defun caller ()
  (compiler-macrolet ((f (x) (f x)))
    (f 1))
  (f 2))";
    let error = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("b"),
        argument: "0".to_owned(),
        call_paths: vec![path("1.3.2")],
        all_calls: false,
        insert: FunctionParameterInsert::End,
        section: FunctionParameterSection::Auto,
    })
    .expect_err("compiler-macrolet body local call should be rejected");

    assert!(
        error
            .to_string()
            .contains("shadowed by a local callable binding or overlaps the selected definition")
    );
}

#[test]
fn rejects_explicit_cl_user_compiler_macrolet_shadowed_call_path() {
    let input = "\
(defun f (a) a)
(defun caller ()
  (cl-user:compiler-macrolet ((f (x) (f x)))
    (f 1))
  (f 2))";
    let error = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("b"),
        argument: "0".to_owned(),
        call_paths: vec![path("1.3.2")],
        all_calls: false,
        insert: FunctionParameterInsert::End,
        section: FunctionParameterSection::Auto,
    })
    .expect_err("qualified compiler-macrolet body local call should be rejected");

    assert!(
        error
            .to_string()
            .contains("shadowed by a local callable binding or overlaps the selected definition")
    );
}
