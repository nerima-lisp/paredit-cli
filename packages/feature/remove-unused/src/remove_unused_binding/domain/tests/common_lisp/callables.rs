use super::super::*;

#[test]
fn plans_unused_macrolet_without_counting_expander_body_reference() {
    let input = "(macrolet ((value (x) (compute value x)) (used (y) (list y))) (list used))";
    let plan = common_lisp_plan(input, Some("value"), false, true);

    assert_eq!(plan.form, "macrolet");
    assert_eq!(plan.binding_name.as_deref(), Some("value"));
    assert_eq!(plan.binding_value.as_deref(), Some("(x) (compute value x)"));
    assert_eq!(plan.reference_count, Some(0));
    assert_eq!(
        plan.replacement,
        "(macrolet ((used (y)\n             (list y)))\n  (list used))"
    );
}

#[test]
fn plans_unused_cl_macrolet_without_counting_expander_body_reference() {
    let input = "(cl:macrolet ((value (x) (compute value x)) (used (y) (list y))) (list used))";
    let plan = common_lisp_plan(input, Some("value"), false, true);

    assert_eq!(plan.form, "cl:macrolet");
    assert_eq!(plan.binding_name.as_deref(), Some("value"));
    assert_eq!(plan.binding_value.as_deref(), Some("(x) (compute value x)"));
    assert_eq!(plan.reference_count, Some(0));
    assert_eq!(
        plan.replacement,
        "(cl:macrolet ((used (y)\n                (list y)))\n  (list used))"
    );
}

#[test]
fn plans_unused_cl_user_macrolet_without_counting_expander_body_reference() {
    let input =
        "(cl-user:macrolet ((value (x) (compute value x)) (used (y) (list y))) (list used))";
    let plan = common_lisp_plan(input, Some("value"), false, true);

    assert_eq!(plan.form, "cl-user:macrolet");
    assert_eq!(plan.binding_name.as_deref(), Some("value"));
    assert_eq!(plan.binding_value.as_deref(), Some("(x) (compute value x)"));
    assert_eq!(plan.reference_count, Some(0));
    assert_eq!(
        plan.replacement,
        "(cl-user:macrolet ((used (y)\n                     (list y)))\n  (list used))"
    );
}

#[test]
fn rejects_referenced_macrolet_binding() {
    let input = "(macrolet ((value (x) (compute x))) (list (value 1)))";
    let error = common_lisp_error(input, Some("value"), false, true);

    assert!(error.contains("zero in-scope references"));
}

#[test]
fn rejects_referenced_common_lisp_cl_user_macrolet_binding() {
    let input = "(cl-user:macrolet ((value (x) (compute x))) (list (value 1)))";
    let error = common_lisp_error(input, Some("value"), false, true);

    assert!(error.contains("zero in-scope references"));
}

#[test]
fn plans_unused_cl_compiler_macrolet_without_counting_expander_body_reference() {
    let input =
        "(cl:compiler-macrolet ((value (x) (compute value x)) (used (y) (list y))) (list used))";
    let plan = common_lisp_plan(input, Some("value"), false, true);

    assert_eq!(plan.form, "cl:compiler-macrolet");
    assert_eq!(plan.binding_name.as_deref(), Some("value"));
    assert_eq!(plan.binding_value.as_deref(), Some("(x) (compute value x)"));
    assert_eq!(plan.reference_count, Some(0));
    assert_eq!(
        plan.replacement,
        "(cl:compiler-macrolet ((used (y)\n                         (list y)))\n  (list used))"
    );
}

#[test]
fn plans_unused_cl_user_compiler_macrolet_without_counting_expander_body_reference() {
    let input = "(cl-user:compiler-macrolet ((value (x) (compute value x)) (used (y) (list y))) (list used))";
    let plan = common_lisp_plan(input, Some("value"), false, true);

    assert_eq!(plan.form, "cl-user:compiler-macrolet");
    assert_eq!(plan.binding_name.as_deref(), Some("value"));
    assert_eq!(plan.binding_value.as_deref(), Some("(x) (compute value x)"));
    assert_eq!(plan.reference_count, Some(0));
    assert_eq!(
        plan.replacement,
        "(cl-user:compiler-macrolet ((used (y)\n                              (list y)))\n  (list used))"
    );
}

#[test]
fn plans_unused_compiler_macrolet_without_counting_expander_body_reference() {
    let input =
        "(compiler-macrolet ((value (x) (compute value x)) (used (y) (list y))) (list used))";
    let plan = common_lisp_plan(input, Some("value"), false, true);

    assert_eq!(plan.form, "compiler-macrolet");
    assert_eq!(plan.binding_name.as_deref(), Some("value"));
    assert_eq!(plan.binding_value.as_deref(), Some("(x) (compute value x)"));
    assert_eq!(plan.reference_count, Some(0));
    assert_eq!(
        plan.replacement,
        "(compiler-macrolet ((used (y)\n                      (list y)))\n  (list used))"
    );
}

#[test]
fn rejects_referenced_compiler_macrolet_binding() {
    let input = "(compiler-macrolet ((value (x) (compute x))) (list (value 1)))";
    let error = common_lisp_error(input, Some("value"), false, true);

    assert!(error.contains("zero in-scope references"));
}

#[test]
fn rejects_referenced_common_lisp_cl_user_compiler_macrolet_binding() {
    let input = "(cl-user:compiler-macrolet ((value (x) (compute x))) (list (value 1)))";
    let error = common_lisp_error(input, Some("value"), false, true);

    assert!(error.contains("zero in-scope references"));
}

#[test]
fn plans_unused_flet_binding_ignoring_definition_body_reference() {
    let input = "(flet ((unused () (unused)) (used () (used))) (used))";
    let plan = common_lisp_plan(input, Some("unused"), false, true);

    assert_eq!(plan.form, "flet");
    assert_eq!(plan.binding_name.as_deref(), Some("unused"));
    assert_eq!(plan.reference_count, Some(0));
    assert_eq!(
        plan.replacement,
        "(flet ((used ()\n         (used)))\n  (used))"
    );
}

#[test]
fn rejects_referenced_flet_binding() {
    let input = "(flet ((unused () 1)) (unused))";
    let error = common_lisp_error(input, Some("unused"), false, true);

    assert!(error.contains("zero in-scope references"));
}

#[test]
fn rejects_recursive_labels_binding() {
    let input = "(labels ((unused () (unused)) (used () (list used))) (used))";
    let error = common_lisp_error(input, Some("unused"), false, true);

    assert!(error.contains("zero in-scope references"));
}
