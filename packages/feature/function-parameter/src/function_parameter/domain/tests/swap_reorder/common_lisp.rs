use super::*;

#[test]
fn swaps_common_lisp_optional_parameters_and_call_arguments_within_section() {
    let input = "(defun f (a &optional b c) (list a b c))\n(print (f 1 2 3))";
    let plan = plan_swap_function_parameters(SwapFunctionParametersRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        left_name: symbol("b"),
        right_name: symbol("c"),
        call_paths: vec![path("1.1")],
        all_calls: false,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defun f (a &optional c b) (list a b c))\n(print (f 1 3 2))"
    );
    assert_swapped_arguments(&plan.swapped_arguments, &[("2", "3")]);
}

#[test]
fn swaps_parameters_within_common_lisp_macrolet_binding_and_call_arguments() {
    let input = "(defun outer ()\n  (macrolet ((with-log (message context) `(list ,message ,context)))\n    (with-log \"hello\" :ctx)))";
    let plan = plan_swap_function_parameters(SwapFunctionParametersRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0.3.1.0"),
        left_name: symbol("message"),
        right_name: symbol("context"),
        call_paths: vec![path("0.3.2")],
        all_calls: false,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defun outer ()\n  (macrolet ((with-log (context message) `(list ,message ,context)))\n    (with-log :ctx \"hello\")))"
    );
    assert_swapped_arguments(&plan.swapped_arguments, &[("\"hello\"", ":ctx")]);
}

#[test]
fn swaps_parameters_within_common_lisp_cl_user_macrolet_binding_and_call_arguments() {
    let input = "(defun outer ()\n  (cl-user:macrolet ((with-log (message context) `(list ,message ,context)))\n    (with-log \"hello\" :ctx)))";
    let plan = plan_swap_function_parameters(SwapFunctionParametersRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0.3.1.0"),
        left_name: symbol("message"),
        right_name: symbol("context"),
        call_paths: vec![path("0.3.2")],
        all_calls: false,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defun outer ()\n  (cl-user:macrolet ((with-log (context message) `(list ,message ,context)))\n    (with-log :ctx \"hello\")))"
    );
    assert_swapped_arguments(&plan.swapped_arguments, &[("\"hello\"", ":ctx")]);
}

#[test]
fn swaps_parameters_within_common_lisp_compiler_macrolet_binding_and_call_arguments() {
    let input = "(defun outer ()\n  (compiler-macrolet ((with-log (message context) `(list ,message ,context)))\n    (with-log \"hello\" :ctx)))";
    let plan = plan_swap_function_parameters(SwapFunctionParametersRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0.3.1.0"),
        left_name: symbol("message"),
        right_name: symbol("context"),
        call_paths: vec![path("0.3.2")],
        all_calls: false,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defun outer ()\n  (compiler-macrolet ((with-log (context message) `(list ,message ,context)))\n    (with-log :ctx \"hello\")))"
    );
    assert_swapped_arguments(&plan.swapped_arguments, &[("\"hello\"", ":ctx")]);
}

#[test]
fn swaps_parameters_within_common_lisp_cl_user_compiler_macrolet_binding_and_call_arguments() {
    let input = "(defun outer ()\n  (cl-user:compiler-macrolet ((with-log (message context) `(list ,message ,context)))\n    (with-log \"hello\" :ctx)))";
    let plan = plan_swap_function_parameters(SwapFunctionParametersRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0.3.1.0"),
        left_name: symbol("message"),
        right_name: symbol("context"),
        call_paths: vec![path("0.3.2")],
        all_calls: false,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defun outer ()\n  (cl-user:compiler-macrolet ((with-log (context message) `(list ,message ,context)))\n    (with-log :ctx \"hello\")))"
    );
    assert_swapped_arguments(&plan.swapped_arguments, &[("\"hello\"", ":ctx")]);
}

#[test]
fn swaps_common_lisp_key_parameters_and_call_arguments_within_section() {
    let input = "(defun f (a &key b c) (list a b c))\n(print (f 1 :b 2 :c 3))";
    let plan = plan_swap_function_parameters(SwapFunctionParametersRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        left_name: symbol("b"),
        right_name: symbol("c"),
        call_paths: vec![path("1.1")],
        all_calls: false,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defun f (a &key c b) (list a b c))\n(print (f 1 :c 3 :b 2))"
    );
    assert_swapped_arguments(&plan.swapped_arguments, &[("2", "3")]);
}

#[test]
fn swaps_parameters_within_common_lisp_define_setf_expander_call_reports_indices() {
    let input = "\
(define-setf-expander access (object mode extra)
  (values nil nil nil `(setf-helper ,object)))
(setf (access item :rw :fast) 1)";
    let plan = plan_swap_function_parameters(SwapFunctionParametersRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        left_name: symbol("mode"),
        right_name: symbol("extra"),
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
    assert_swapped_arguments(&plan.swapped_arguments, &[(":rw", ":fast")]);
}

#[test]
fn swaps_parameters_within_common_lisp_long_form_defsetf_call() {
    let input = "\
(defsetf access (object mode extra) (value)
  `(update-access ,object ,value))
(setf (access item :rw :fast) 1)";
    let plan = plan_swap_function_parameters(SwapFunctionParametersRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        left_name: symbol("mode"),
        right_name: symbol("extra"),
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
    assert_swapped_arguments(&plan.swapped_arguments, &[(":rw", ":fast")]);
}

#[test]
fn swaps_parameters_within_common_lisp_define_modify_macro_call() {
    let input = "\
(define-modify-macro appendf (item tail extra) append)
(appendf place 10 20 30)";
    let plan = plan_swap_function_parameters(SwapFunctionParametersRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        left_name: symbol("tail"),
        right_name: symbol("extra"),
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
    assert_swapped_arguments(&plan.swapped_arguments, &[("20", "30")]);
}

#[test]
fn reorders_parameters_within_common_lisp_labels_binding_and_call_arguments() {
    let input = "(defun outer ()\n  (labels ((render (width height margin) (list width height margin)))\n    (render 10 20 5)))";
    let plan = plan_reorder_function_parameters(ReorderFunctionParametersRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0.3.1.0"),
        parameter_order: symbol_names(&["margin", "width", "height"]),
        call_paths: vec![path("0.3.2")],
        all_calls: false,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defun outer ()\n  (labels ((render (margin width height) (list width height margin)))\n    (render 5 10 20)))"
    );
    assert_parameter_order(&plan.old_parameter_order, &["width", "height", "margin"]);
    assert_parameter_order(&plan.new_parameter_order, &["margin", "width", "height"]);
    assert_reordered_arguments(&plan.reordered_arguments, &[&["5", "10", "20"]]);
}

#[test]
fn reorders_parameters_within_common_lisp_macrolet_binding_and_call_arguments() {
    let input = "(defun outer ()\n  (macrolet ((render (width height margin) `(list ,width ,height ,margin)))\n    (render 10 20 5)))";
    let plan = plan_reorder_function_parameters(ReorderFunctionParametersRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0.3.1.0"),
        parameter_order: symbol_names(&["margin", "width", "height"]),
        call_paths: vec![path("0.3.2")],
        all_calls: false,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defun outer ()\n  (macrolet ((render (margin width height) `(list ,width ,height ,margin)))\n    (render 5 10 20)))"
    );
    assert_parameter_order(&plan.old_parameter_order, &["width", "height", "margin"]);
    assert_parameter_order(&plan.new_parameter_order, &["margin", "width", "height"]);
    assert_reordered_arguments(&plan.reordered_arguments, &[&["5", "10", "20"]]);
}

#[test]
fn reorders_parameters_within_common_lisp_cl_user_macrolet_binding_and_call_arguments() {
    let input = "(defun outer ()\n  (cl-user:macrolet ((render (width height margin) `(list ,width ,height ,margin)))\n    (render 10 20 5)))";
    let plan = plan_reorder_function_parameters(ReorderFunctionParametersRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0.3.1.0"),
        parameter_order: symbol_names(&["margin", "width", "height"]),
        call_paths: vec![path("0.3.2")],
        all_calls: false,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defun outer ()\n  (cl-user:macrolet ((render (margin width height) `(list ,width ,height ,margin)))\n    (render 5 10 20)))"
    );
    assert_parameter_order(&plan.old_parameter_order, &["width", "height", "margin"]);
    assert_parameter_order(&plan.new_parameter_order, &["margin", "width", "height"]);
    assert_reordered_arguments(&plan.reordered_arguments, &[&["5", "10", "20"]]);
}

#[test]
fn reorders_parameters_within_common_lisp_compiler_macrolet_binding_and_call_arguments() {
    let input = "(defun outer ()\n  (compiler-macrolet ((render (width height margin) `(list ,width ,height ,margin)))\n    (render 10 20 5)))";
    let plan = plan_reorder_function_parameters(ReorderFunctionParametersRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0.3.1.0"),
        parameter_order: symbol_names(&["margin", "width", "height"]),
        call_paths: vec![path("0.3.2")],
        all_calls: false,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defun outer ()\n  (compiler-macrolet ((render (margin width height) `(list ,width ,height ,margin)))\n    (render 5 10 20)))"
    );
    assert_parameter_order(&plan.old_parameter_order, &["width", "height", "margin"]);
    assert_parameter_order(&plan.new_parameter_order, &["margin", "width", "height"]);
    assert_reordered_arguments(&plan.reordered_arguments, &[&["5", "10", "20"]]);
}

#[test]
fn reorders_parameters_within_common_lisp_cl_user_compiler_macrolet_binding_and_call_arguments() {
    let input = "(defun outer ()\n  (cl-user:compiler-macrolet ((render (width height margin) `(list ,width ,height ,margin)))\n    (render 10 20 5)))";
    let plan = plan_reorder_function_parameters(ReorderFunctionParametersRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0.3.1.0"),
        parameter_order: symbol_names(&["margin", "width", "height"]),
        call_paths: vec![path("0.3.2")],
        all_calls: false,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defun outer ()\n  (cl-user:compiler-macrolet ((render (margin width height) `(list ,width ,height ,margin)))\n    (render 5 10 20)))"
    );
    assert_parameter_order(&plan.old_parameter_order, &["width", "height", "margin"]);
    assert_parameter_order(&plan.new_parameter_order, &["margin", "width", "height"]);
    assert_reordered_arguments(&plan.reordered_arguments, &[&["5", "10", "20"]]);
}

#[test]
fn reorders_parameters_within_common_lisp_define_setf_expander_call() {
    let input = "\
(define-setf-expander access (object mode extra)
  (values nil nil nil `(setf-helper ,object)))
(setf (access item :rw :fast) 1)";
    let plan = plan_reorder_function_parameters(ReorderFunctionParametersRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        parameter_order: symbol_names(&["object", "extra", "mode"]),
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
    assert_parameter_order(&plan.old_parameter_order, &["object", "mode", "extra"]);
    assert_parameter_order(&plan.new_parameter_order, &["object", "extra", "mode"]);
    assert_reordered_arguments(&plan.reordered_arguments, &[&["item", ":fast", ":rw"]]);
}

#[test]
fn reorders_parameters_within_cl_qualified_define_setf_expander_call() {
    let input = "\
(cl:define-setf-expander access (object mode extra)
  (values nil nil nil `(setf-helper ,object)))
(setf (access item :rw :fast) 1)";
    let plan = plan_reorder_function_parameters(ReorderFunctionParametersRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        parameter_order: symbol_names(&["object", "extra", "mode"]),
        call_paths: vec![path("1")],
        all_calls: false,
    })
    .expect("plan");

    assert_eq!(plan.call_paths, vec![path("1")]);
    assert_eq!(
        plan.rewritten,
        "\
(cl:define-setf-expander access (object extra mode)
  (values nil nil nil `(setf-helper ,object)))
(setf (access item :fast :rw) 1)"
    );
    assert_parameter_order(&plan.old_parameter_order, &["object", "mode", "extra"]);
    assert_parameter_order(&plan.new_parameter_order, &["object", "extra", "mode"]);
    assert_reordered_arguments(&plan.reordered_arguments, &[&["item", ":fast", ":rw"]]);
}

#[test]
fn swaps_parameters_within_common_lisp_define_setf_expander_call() {
    let input = "\
(define-setf-expander access (object mode extra)
  (values nil nil nil `(setf-helper ,object)))
(setf (access item :rw :fast) 1)";
    let plan = plan_swap_function_parameters(SwapFunctionParametersRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        left_name: symbol("mode"),
        right_name: symbol("extra"),
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
    assert_eq!(plan.left_name.as_str(), "mode");
    assert_eq!(plan.right_name.as_str(), "extra");
    assert_eq!(plan.left_index, 1);
    assert_eq!(plan.right_index, 2);
    assert_swapped_arguments(&plan.swapped_arguments, &[(":rw", ":fast")]);
}

#[test]
fn reorders_parameters_within_common_lisp_long_form_defsetf_call() {
    let input = "\
(defsetf access (object mode extra) (value)
  `(update-access ,object ,value))
(setf (access item :rw :fast) 1)";
    let plan = plan_reorder_function_parameters(ReorderFunctionParametersRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        parameter_order: symbol_names(&["object", "extra", "mode"]),
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
    assert_parameter_order(&plan.old_parameter_order, &["object", "mode", "extra"]);
    assert_parameter_order(&plan.new_parameter_order, &["object", "extra", "mode"]);
    assert_reordered_arguments(&plan.reordered_arguments, &[&["item", ":fast", ":rw"]]);
}

#[test]
fn reorders_parameters_within_common_lisp_define_modify_macro_call() {
    let input = "\
(define-modify-macro appendf (item tail extra) append)
(appendf place 10 20 30)";
    let plan = plan_reorder_function_parameters(ReorderFunctionParametersRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        parameter_order: symbol_names(&["item", "extra", "tail"]),
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
    assert_parameter_order(&plan.old_parameter_order, &["item", "tail", "extra"]);
    assert_parameter_order(&plan.new_parameter_order, &["item", "extra", "tail"]);
    assert_reordered_arguments(&plan.reordered_arguments, &[&["10", "30", "20"]]);
}

#[test]
fn reorders_parameters_within_cl_qualified_define_modify_macro_call() {
    let input = "\
(cl:define-modify-macro appendf (item tail extra) append)
(appendf place 10 20 30)";
    let plan = plan_reorder_function_parameters(ReorderFunctionParametersRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        parameter_order: symbol_names(&["item", "extra", "tail"]),
        call_paths: vec![path("1")],
        all_calls: false,
    })
    .expect("plan");

    assert_eq!(plan.call_paths, vec![path("1")]);
    assert_eq!(
        plan.rewritten,
        "\
(cl:define-modify-macro appendf (item extra tail) append)
(appendf place 10 30 20)"
    );
    assert_parameter_order(&plan.old_parameter_order, &["item", "tail", "extra"]);
    assert_parameter_order(&plan.new_parameter_order, &["item", "extra", "tail"]);
    assert_reordered_arguments(&plan.reordered_arguments, &[&["10", "30", "20"]]);
}

#[test]
fn reorders_common_lisp_defmethod_specialized_parameters_and_call_arguments() {
    let input = "(defmethod render :around ((node widget) stream style) (draw node stream style))\n(render thing out :fancy)";
    let plan = plan_reorder_function_parameters(ReorderFunctionParametersRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        parameter_order: symbol_names(&["style", "node", "stream"]),
        call_paths: vec![path("1")],
        all_calls: false,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defmethod render :around (style (node widget) stream) (draw node stream style))\n(render :fancy thing out)"
    );
    assert_parameter_order(&plan.old_parameter_order, &["node", "stream", "style"]);
    assert_reordered_arguments(&plan.reordered_arguments, &[&[":fancy", "thing", "out"]]);
}

#[test]
fn reorders_common_lisp_defgeneric_parameters_and_call_arguments() {
    let input = "(defgeneric render (node stream context))\n(defmethod render ((node widget) stream) (draw node stream))\n(render thing out :fancy)";
    let plan = plan_reorder_function_parameters(ReorderFunctionParametersRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        parameter_order: symbol_names(&["context", "node", "stream"]),
        call_paths: vec![path("2")],
        all_calls: false,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defgeneric render (context node stream))\n(defmethod render ((node widget) stream) (draw node stream))\n(render :fancy thing out)"
    );
    assert_parameter_order(&plan.old_parameter_order, &["node", "stream", "context"]);
    assert_parameter_order(&plan.new_parameter_order, &["context", "node", "stream"]);
    assert_reordered_arguments(&plan.reordered_arguments, &[&[":fancy", "thing", "out"]]);
}

#[test]
fn reorders_common_lisp_macro_required_parameters_before_body_marker() {
    let input = "(defmacro wrap (left right &body body) `(list ,left ,right ,@body))\n(wrap 1 2 (+ 3 4) (+ 5 6))";
    let plan = plan_reorder_function_parameters(ReorderFunctionParametersRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        parameter_order: symbol_names(&["right", "left"]),
        call_paths: vec![path("1")],
        all_calls: false,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defmacro wrap (right left &body body) `(list ,left ,right ,@body))\n(wrap 2 1 (+ 3 4) (+ 5 6))"
    );
    assert_reordered_arguments(
        &plan.reordered_arguments,
        &[&["2", "1", "(+ 3 4)", "(+ 5 6)"]],
    );
}
