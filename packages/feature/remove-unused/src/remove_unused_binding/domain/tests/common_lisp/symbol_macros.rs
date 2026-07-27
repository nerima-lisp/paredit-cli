use super::super::*;

#[test]
fn plans_unused_symbol_macrolet_without_counting_expansion_reference() {
    let input = "(symbol-macrolet ((value (compute value)) (used other)) (list used))";
    let plan = common_lisp_plan(input, Some("value"), false, true);

    assert_eq!(plan.form, "symbol-macrolet");
    assert_eq!(plan.binding_name.as_deref(), Some("value"));
    assert_eq!(plan.binding_value.as_deref(), Some("(compute value)"));
    assert_eq!(plan.reference_count, Some(0));
    assert_eq!(
        plan.replacement,
        "(symbol-macrolet ((used other))\n  (list used))"
    );
}

#[test]
fn plans_emacs_lisp_unused_cl_symbol_macrolet_without_counting_expansion_reference() {
    let input = "(cl-symbol-macrolet ((value (compute value)) (used other)) (list used))";
    let plan =
        plan_remove_unused_binding_for(input, Dialect::EmacsLisp, None, Some("value"), false, true);

    assert_eq!(plan.form, "cl-symbol-macrolet");
    assert_eq!(plan.binding_name.as_deref(), Some("value"));
    assert_eq!(plan.binding_value.as_deref(), Some("(compute value)"));
    assert_eq!(plan.reference_count, Some(0));
    assert_eq!(
        plan.replacement,
        "(cl-symbol-macrolet ((used other))\n  (list used))"
    );
}

#[test]
fn plans_common_lisp_unused_cl_user_symbol_macrolet_without_counting_expansion_reference() {
    let input = "(cl-user:symbol-macrolet ((value (compute value)) (used other)) (list used))";
    let plan = common_lisp_plan(input, Some("value"), false, true);

    assert_eq!(plan.form, "cl-user:symbol-macrolet");
    assert_eq!(plan.binding_name.as_deref(), Some("value"));
    assert_eq!(plan.binding_value.as_deref(), Some("(compute value)"));
    assert_eq!(plan.reference_count, Some(0));
    assert_eq!(
        plan.replacement,
        "(cl-user:symbol-macrolet ((used other))\n  (list used))"
    );
}

#[test]
fn rejects_referenced_symbol_macrolet_binding() {
    let input = "(symbol-macrolet ((value (compute))) (list value))";
    let error = common_lisp_error(input, Some("value"), false, true);

    assert!(error.contains("zero in-scope references"));
}

#[test]
fn rejects_referenced_common_lisp_cl_user_symbol_macrolet_binding() {
    let input = "(cl-user:symbol-macrolet ((value (compute))) (list value))";
    let error = common_lisp_error(input, Some("value"), false, true);

    assert!(error.contains("zero in-scope references"));
}
