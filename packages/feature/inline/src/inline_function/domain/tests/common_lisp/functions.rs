use super::super::*;

#[test]
fn plans_single_common_lisp_call() {
    let input = "(defun area (w h) (* w h))\n(print (area 3 4))";
    let plan = inline_plan(input);

    assert_eq!(plan.function_name.as_str(), "area");
    assert_eq!(plan.calls[0].replacement, "(* 3 4)");
    assert_eq!(
        plan.rewritten,
        "(defun area (w h) (* w h))\n(print (* 3 4))"
    );
    assert!(plan.changed);
}

#[test]
fn discovers_all_calls_and_removes_definition() {
    let input = "(defun inc (x) (+ x 1))\n(print (inc 1))\n(print (inc 2))";
    let plan = remove_definition_plan(input, Dialect::CommonLisp);

    assert_eq!(plan.calls.len(), 2);
    assert_eq!(plan.rewritten, "(print (+ 1 1))\n(print (+ 2 1))");
    assert!(plan.definition_removed);
}

#[test]
fn rejects_removing_a_definition_with_a_comment() {
    let input = "(defun inc (x)\n  ;; add one\n  (+ x 1))\n(print (inc 1))\n(print (inc 2))";
    let err = plan_inline_function(InlineFunctionRequest {
        remove_definition: true,
        ..all_calls_request(input, Dialect::CommonLisp)
    })
    .expect_err("a comment in a removed definition must not be silently discarded");

    assert!(err.to_string().contains("comment"));
}

#[test]
fn inlining_without_removing_definition_keeps_its_comment_intact() {
    let input = "(defun inc (x)\n  ;; add one\n  (+ x 1))\n(print (inc 1))";
    let plan = inline_plan(input);

    assert_eq!(plan.calls[0].replacement, "(+ 1 1)");
    assert!(plan.rewritten.contains(";; add one"));
}

#[test]
fn inlines_common_lisp_optional_parameter_when_argument_is_supplied() {
    let input = "(defun add-default (x &optional (y 10)) (+ x y))\n(print (add-default 1 2))";
    let plan = inline_plan(input);

    assert_eq!(plan.calls[0].replacement, "(+ 1 2)");
    assert_eq!(
        plan.rewritten,
        "(defun add-default (x &optional (y 10)) (+ x y))\n(print (+ 1 2))"
    );
}

#[test]
fn inlines_common_lisp_optional_parameter_default_when_argument_is_missing() {
    let input = "(defun add-default (x &optional (y 10)) (+ x y))\n(print (add-default 1))";
    let plan = inline_plan(input);

    assert_eq!(parameter(&plan.calls[0], "y").argument, "10");
    assert_eq!(plan.calls[0].replacement, "(+ 1 10)");
    assert_eq!(
        plan.rewritten,
        "(defun add-default (x &optional (y 10)) (+ x y))\n(print (+ 1 10))"
    );
}

#[test]
fn inlines_common_lisp_optional_supplied_p_parameter() {
    let input = "(defun maybe (x &optional (y 10 y-p)) (if y-p y x))\n(print (maybe 1 2))";
    let plan = inline_plan(input);

    assert_eq!(parameter(&plan.calls[0], "y-p").argument, "t");
    assert_eq!(plan.calls[0].replacement, "(if t 2 1)");
    assert_eq!(
        plan.rewritten,
        "(defun maybe (x &optional (y 10 y-p)) (if y-p y x))\n(print (if t 2 1))"
    );
}

#[test]
fn inlines_common_lisp_key_parameter_when_argument_is_supplied() {
    let input =
        "(defun render (x &key (style :plain)) (list x style))\n(print (render 1 :style :bold))";
    let plan = inline_plan(input);

    assert_eq!(plan.calls[0].replacement, "(list 1 :bold)");
    assert_eq!(
        plan.rewritten,
        "(defun render (x &key (style :plain)) (list x style))\n(print (list 1 :bold))"
    );
    assert_eq!(plan.calls[0].parameters[1].name, "style");
    assert_eq!(plan.calls[0].parameters[1].argument, ":bold");
}

#[test]
fn inlines_common_lisp_external_key_parameter_designator() {
    let input = "(defun render (x &key ((:external internal) 10)) (list x internal))\n(print (render 1 :external 20))";
    let plan = inline_plan(input);

    assert_eq!(plan.calls[0].replacement, "(list 1 20)");
    assert_eq!(
        plan.rewritten,
        "(defun render (x &key ((:external internal) 10)) (list x internal))\n(print (list 1 20))"
    );
}

#[test]
fn inlines_common_lisp_key_parameter_default_when_argument_is_missing() {
    let input = "(defun render (x &key (style :plain)) (list x style))\n(print (render 1))";
    let plan = inline_plan(input);

    assert_eq!(parameter(&plan.calls[0], "style").argument, ":plain");
    assert_eq!(plan.calls[0].replacement, "(list 1 :plain)");
    assert_eq!(
        plan.rewritten,
        "(defun render (x &key (style :plain)) (list x style))\n(print (list 1 :plain))"
    );
}

#[test]
fn rejects_common_lisp_key_parameter_with_duplicate_argument() {
    let input = "(defun render (x &key style) (list x style))\n(print (render 1 :style :bold :style :plain))";
    let error = inline_error(input, "duplicate key argument must fail");

    assert!(error.to_string().contains("duplicate keyword :style"));
}

#[test]
fn inlines_common_lisp_key_supplied_p_parameter() {
    let input = "(defun render (x &key (style :plain style-p)) (if style-p style x))\n(print (render 1 :style :bold))";
    let plan = inline_plan(input);

    assert_eq!(parameter(&plan.calls[0], "style-p").argument, "t");
    assert_eq!(plan.calls[0].replacement, "(if t :bold 1)");
    assert_eq!(
        plan.rewritten,
        "(defun render (x &key (style :plain style-p)) (if style-p style x))\n(print (if t :bold 1))"
    );
}

#[test]
fn discovers_all_calls_skips_common_lisp_flet_body_local_callable_calls() {
    let input = "(defun helper (x) (+ x 1))\n(defun render () (flet ((helper (x) (helper x))) (helper 2)) (helper 3))";
    let plan = all_calls_plan(input, Dialect::CommonLisp);

    assert_eq!(
        plan.call_paths
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        vec!["1.3.1.0.2", "1.4"]
    );
    assert_eq!(
        plan.rewritten,
        "(defun helper (x) (+ x 1))\n(defun render () (flet ((helper (x) (+ x 1))) (helper 2)) (+ 3 1))"
    );
}

#[test]
fn discovers_all_calls_skips_common_lisp_labels_local_callable_calls() {
    let input = "(defun helper (x) (+ x 1))\n(defun render () (labels ((helper (x) (if x (helper nil) 0))) (helper t)) (helper 3))";
    let plan = all_calls_plan(input, Dialect::CommonLisp);

    assert_eq!(
        plan.call_paths
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        vec!["1.4"]
    );
    assert_eq!(
        plan.rewritten,
        "(defun helper (x) (+ x 1))\n(defun render () (labels ((helper (x) (if x (helper nil) 0))) (helper t)) (+ 3 1))"
    );
}

#[test]
fn rejects_duplicate_evaluation_by_default() {
    let input = "(defun twice (x) (+ x x))\n(print (twice (next)))";
    let error = inline_error(input, "duplicate evaluation");

    assert!(error.to_string().contains("duplicate argument"));
}

#[test]
fn ignores_shadowed_parameter_references() {
    let input = "(defun outer (x) (let ((x 10)) x))\n(print (outer (next)))";
    let error = inline_error(input, "dropped shadowed argument");

    assert!(error.to_string().contains("drop argument"));
}
