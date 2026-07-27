use super::super::*;

#[test]
fn plans_common_lisp_single_unused_binding() {
    let input = "(let ((unused 1) (used 2)) used)";
    let plan = plan_remove_unused_binding_for(
        input,
        Dialect::CommonLisp,
        Some("0"),
        Some("unused"),
        false,
        false,
    );

    assert_eq!(plan.binding_name.as_deref(), Some("unused"));
    assert_eq!(plan.binding_value.as_deref(), Some("1"));
    assert_eq!(plan.reference_count, Some(0));
    assert_eq!(plan.replacement, "(let ((used 2))\n  used)");
    assert_eq!(plan.rewritten, "(let ((used 2))\n  used)");
    assert!(plan.dropped_value_requires_review);
    assert!(plan.changed);
}

#[test]
fn plans_unqualified_name_for_package_qualified_common_lisp_binding() {
    let input = "(let ((cl-user:unused 1) (used 2)) used)";
    let plan = plan_remove_unused_binding_for(
        input,
        Dialect::CommonLisp,
        Some("0"),
        Some("unused"),
        false,
        false,
    );

    assert_eq!(plan.binding_name.as_deref(), Some("cl-user:unused"));
    assert_eq!(plan.binding_value.as_deref(), Some("1"));
    assert_eq!(plan.reference_count, Some(0));
    assert_eq!(plan.replacement, "(let ((used 2))\n  used)");
    assert_eq!(plan.rewritten, "(let ((used 2))\n  used)");
    assert!(plan.dropped_value_requires_review);
    assert!(plan.changed);
}

#[test]
fn rejects_referenced_binding() {
    let input = "(let ((x 1)) x)";
    let error = common_lisp_error(input, Some("x"), false, true);

    assert!(error.contains("zero in-scope references"));
}

#[test]
fn rejects_missing_name_when_not_removing_all_bindings() {
    let input = "(let ((unused 1)) 42)";
    let error = common_lisp_error(input, None, false, true);

    assert_eq!(
        error,
        "remove-unused-binding requires --name or --all-bindings"
    );
}

#[test]
fn plans_unused_binding_ignoring_shadowed_lambda_parameter() {
    let input = "(let ((x 1) (used 2)) (list used (lambda (x) x)))";
    let plan = common_lisp_plan(input, Some("x"), false, true);

    assert_eq!(plan.binding_name.as_deref(), Some("x"));
    assert_eq!(plan.reference_count, Some(0));
    assert_eq!(
        plan.replacement,
        "(let ((used 2))\n  (list\n    used\n    (lambda (x)\n      x)))"
    );
}

#[test]
fn rejects_reference_before_shadowed_lambda_parameter() {
    let input = "(let ((x 1)) (list x (lambda (x) x)))";
    let error = common_lisp_error(input, Some("x"), false, true);

    assert!(error.contains("zero in-scope references"));
}

#[test]
fn plans_unused_binding_ignoring_shadowed_inner_let() {
    let input = "(let ((x 1) (used 2)) (let ((x 3)) x) used)";
    let plan = common_lisp_plan(input, Some("x"), false, true);

    assert_eq!(plan.binding_name.as_deref(), Some("x"));
    assert_eq!(plan.reference_count, Some(0));
    assert_eq!(
        plan.replacement,
        "(let ((used 2))\n  (let ((x 3))\n    x)\n  used)"
    );
}

#[test]
fn plans_unused_binding_ignoring_shadowed_dolist_variable() {
    let input = "(let ((value 1) (used 2)) (list used (dolist (value items value) value)))";
    let plan = common_lisp_plan(input, Some("value"), false, true);

    assert_eq!(plan.binding_name.as_deref(), Some("value"));
    assert_eq!(plan.reference_count, Some(0));
    assert_eq!(
        plan.replacement,
        "(let ((used 2))\n  (list\n    used\n    (dolist (value items value)\n      value)))"
    );
}

#[test]
fn plans_unused_binding_ignoring_shadowed_with_slots_variable() {
    let input = "(let ((value 1) (used 2)) (list used (with-slots (value) object value)))";
    let plan = common_lisp_plan(input, Some("value"), false, true);

    assert_eq!(plan.binding_name.as_deref(), Some("value"));
    assert_eq!(plan.reference_count, Some(0));
    assert_eq!(
        plan.replacement,
        "(let ((used 2))\n  (list\n    used\n    (with-slots (value)\n      object\n      value)))"
    );
}

#[test]
fn plans_all_unused_bindings_by_replacing_form_with_body() {
    let input = "(let ((unused 1)) body)";
    let plan = common_lisp_plan(input, None, true, true);

    assert_eq!(plan.bindings.len(), 1);
    assert_eq!(plan.replacement, "body");
    assert_eq!(plan.rewritten, "body");
}

#[test]
fn all_bindings_skips_earmuffed_special_variable_rebind_with_zero_references() {
    // `(let ((*read-eval* nil)) (read stream))` is meaningful purely through
    // its dynamic-scope side effect for the body's dynamic extent — no
    // lexical reference to `*read-eval*` is needed or expected. Bulk
    // `--all-bindings` must not delete it: doing so can silently change
    // program behavior (in this exact shape, reinstating an
    // arbitrary-code-execution risk the binding exists to close).
    let input = "(let ((*read-eval* nil)) (read stream))";
    let error = common_lisp_error(input, None, true, true);

    assert!(error.contains("found no unused bindings"));
}

fn assert_common_lisp_all_bindings_keeps_special_binding(input: &str, path: Option<&str>) {
    let error = remove_unused_binding_error_for(input, Dialect::CommonLisp, path, None, true, true);

    assert!(error.contains("found no unused bindings"));
}

#[test]
fn all_bindings_skips_non_earmuffed_binding_declared_special_by_declaim() {
    assert_common_lisp_all_bindings_keeps_special_binding(
        "(declaim (special dynamic))\n(let ((dynamic 1)) (funcall thunk))",
        Some("1"),
    );
}

#[test]
fn all_bindings_skips_non_earmuffed_binding_declared_special_by_defvar() {
    assert_common_lisp_all_bindings_keeps_special_binding(
        "(defvar dynamic 0)\n(let ((dynamic 1)) (funcall thunk))",
        Some("1"),
    );
}

#[test]
fn all_bindings_skips_non_earmuffed_binding_declared_special_by_defparameter() {
    assert_common_lisp_all_bindings_keeps_special_binding(
        "(defparameter dynamic 0)\n(let ((dynamic 1)) (funcall thunk))",
        Some("1"),
    );
}

#[test]
fn all_bindings_skips_non_earmuffed_binding_declared_special_by_proclaim() {
    assert_common_lisp_all_bindings_keeps_special_binding(
        "(proclaim '(special dynamic))\n(let ((dynamic 1)) (funcall thunk))",
        Some("1"),
    );
}

#[test]
fn all_bindings_skips_non_earmuffed_binding_declared_special_by_locally() {
    assert_common_lisp_all_bindings_keeps_special_binding(
        "(locally (declare (special dynamic)) (let ((dynamic 1)) (funcall thunk)))",
        Some("0.2"),
    );
}

#[test]
fn all_bindings_skips_non_earmuffed_binding_declared_special_in_its_own_body() {
    let input = "(let ((dynamic 1)) (declare (special dynamic)) (funcall thunk))";
    let error = common_lisp_error(input, None, true, true);

    assert!(error.contains("found no unused bindings"));
}

#[test]
fn all_bindings_does_not_protect_local_callable_named_like_special_variable() {
    let input = "(defvar dynamic 0)\n(flet ((dynamic () 1)) body)";
    let plan =
        plan_remove_unused_binding_for(input, Dialect::CommonLisp, Some("1"), None, true, true);

    assert_eq!(plan.bindings[0].binding_name, "dynamic");
    assert_eq!(plan.replacement, "body");
}

#[test]
fn all_bindings_still_removes_a_plain_unused_binding_alongside_a_special_variable_rebind() {
    let input = "(let ((*read-eval* nil) (unused 1)) (read stream))";
    let plan = common_lisp_plan(input, None, true, true);

    assert_eq!(plan.bindings.len(), 1);
    assert_eq!(plan.bindings[0].binding_name, "unused");
    assert_eq!(
        plan.replacement,
        "(let ((*read-eval* nil))\n  (read stream))"
    );
}

#[test]
fn explicit_name_still_removes_an_earmuffed_special_variable_rebind() {
    // `--all-bindings` is a bulk, unattended operation and must stay
    // conservative, but an explicit `--name` target is a deliberate,
    // reviewed choice and should not be second-guessed.
    let input = "(let ((*read-eval* nil)) (read stream))";
    let plan = common_lisp_plan(input, Some("*read-eval*"), false, true);

    assert_eq!(plan.binding_name.as_deref(), Some("*read-eval*"));
    assert_eq!(plan.replacement, "(read stream)");
}

#[test]
fn plans_unused_do_binding_without_counting_init_reference() {
    let input = "(do ((unused (compute unused)) (i 0 (1+ i))) ((>= i limit) i) (print i))";
    let plan = common_lisp_plan(input, Some("unused"), false, true);

    assert_eq!(plan.form, "do");
    assert_eq!(plan.binding_name.as_deref(), Some("unused"));
    assert_eq!(plan.binding_value.as_deref(), Some("(compute unused)"));
    assert_eq!(plan.reference_count, Some(0));
    assert_eq!(
        plan.replacement,
        "(do ((i 0 (1+ i)))\n  ((>= i limit) i)\n  (print i))"
    );
}

#[test]
fn rejects_do_binding_used_in_step() {
    let input = "(do ((unused 0 (1+ unused))) ((done) unused))";
    let error = common_lisp_error(input, Some("unused"), false, true);

    assert!(error.contains("zero in-scope references"));
}

#[test]
fn plans_all_unused_prog_bindings_without_collapsing_form() {
    let input = "(prog ((unused (compute))) (return done))";
    let plan = common_lisp_plan(input, None, true, true);

    assert_eq!(plan.form, "prog");
    assert_eq!(plan.bindings.len(), 1);
    assert_eq!(plan.replacement, "(prog ()\n  (return done))");
}

#[test]
fn plans_common_lisp_qualified_unused_do_binding_without_counting_init_reference() {
    let input = "(cl-user:do ((unused (compute unused)) (i 0 (1+ i))) ((>= i limit) i) (print i))";
    let plan = common_lisp_plan(input, Some("unused"), false, true);

    assert_eq!(plan.form, "cl-user:do");
    assert_eq!(plan.binding_name.as_deref(), Some("unused"));
    assert_eq!(plan.binding_value.as_deref(), Some("(compute unused)"));
    assert_eq!(plan.reference_count, Some(0));
    assert_eq!(
        plan.replacement,
        "(cl-user:do ((i 0 (1+ i)))\n  ((>= i limit) i)\n  (print i))"
    );
}

#[test]
fn rejects_prog_star_binding_used_in_later_init() {
    let input = "(prog* ((seed (make)) (copy seed)) (return copy))";
    let error = common_lisp_error(input, Some("seed"), false, true);

    assert!(error.contains("zero in-scope references"));
}

#[test]
fn rejects_common_lisp_qualified_prog_star_binding_used_in_later_init() {
    let input = "(cl-user:prog* ((seed (make)) (copy seed)) (return copy))";
    let error = common_lisp_error(input, Some("seed"), false, true);

    assert!(error.contains("zero in-scope references"));
}

#[test]
fn rejects_qualified_prog_star_binding_used_in_later_init() {
    let input = "(cl:prog* ((seed (make)) (copy seed)) (return copy))";
    let error = common_lisp_error(input, Some("seed"), false, true);

    assert!(error.contains("zero in-scope references"));
}
