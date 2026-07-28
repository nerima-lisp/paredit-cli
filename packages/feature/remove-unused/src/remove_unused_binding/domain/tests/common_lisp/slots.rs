use super::super::*;

#[test]
fn plans_common_lisp_qualified_with_slots_binding_without_counting_instance_expression() {
    let input = "(cl-user:with-slots ((value slot-name) (used used-slot)) value (list used))";
    let plan = common_lisp_plan(input, Some("value"), false, true);

    assert_eq!(plan.form, "cl-user:with-slots");
    assert_eq!(plan.binding_name.as_deref(), Some("value"));
    assert_eq!(plan.binding_value.as_deref(), Some("slot-name"));
    assert_eq!(plan.reference_count, Some(0));
    assert_eq!(
        plan.replacement,
        "(cl-user:with-slots ((used used-slot))\n  value\n  (list used))"
    );
}

#[test]
fn plans_unused_with_slots_bare_binding_without_counting_instance_expression() {
    let input = "(with-slots (value used) value (list used))";
    let plan = common_lisp_plan(input, Some("value"), false, true);

    assert_eq!(plan.form, "with-slots");
    assert_eq!(plan.binding_name.as_deref(), Some("value"));
    assert_eq!(plan.binding_value.as_deref(), Some("value"));
    assert_eq!(plan.reference_count, Some(0));
    assert_eq!(
        plan.replacement,
        "(with-slots (used)\n  value\n  (list used))"
    );
}

#[test]
fn plans_unused_with_slots_pair_binding() {
    let input = "(with-slots ((local slot-name) used) object (list used))";
    let plan = common_lisp_plan(input, Some("local"), false, true);

    assert_eq!(plan.form, "with-slots");
    assert_eq!(plan.binding_name.as_deref(), Some("local"));
    assert_eq!(plan.binding_value.as_deref(), Some("slot-name"));
    assert_eq!(plan.reference_count, Some(0));
    assert_eq!(
        plan.replacement,
        "(with-slots (used)\n  object\n  (list used))"
    );
}

#[test]
fn rejects_referenced_with_slots_binding() {
    let input = "(with-slots (value) object (list value))";
    let error = common_lisp_error(input, Some("value"), false, true);

    assert!(error.contains("zero in-scope references"));
}

#[test]
fn plans_unused_with_accessors_binding_without_counting_instance_expression() {
    let input = "(with-accessors ((value slot-name) (used used-slot)) value (list used))";
    let plan = common_lisp_plan(input, Some("value"), false, true);

    assert_eq!(plan.form, "with-accessors");
    assert_eq!(plan.binding_name.as_deref(), Some("value"));
    assert_eq!(plan.binding_value.as_deref(), Some("slot-name"));
    assert_eq!(plan.reference_count, Some(0));
    assert_eq!(
        plan.replacement,
        "(with-accessors ((used used-slot))\n  value\n  (list used))"
    );
}

#[test]
fn rejects_referenced_with_accessors_binding() {
    let input = "(with-accessors ((value slot-name)) object (list value))";
    let error = common_lisp_error(input, Some("value"), false, true);

    assert!(error.contains("zero in-scope references"));
}

#[test]
fn plans_common_lisp_qualified_with_accessors_binding_without_counting_instance_expression() {
    let input = "(cl-user:with-accessors ((value slot-name) (used used-slot)) value (list used))";
    let plan = common_lisp_plan(input, Some("value"), false, true);

    assert_eq!(plan.form, "cl-user:with-accessors");
    assert_eq!(plan.binding_name.as_deref(), Some("value"));
    assert_eq!(plan.binding_value.as_deref(), Some("slot-name"));
    assert_eq!(plan.reference_count, Some(0));
    assert_eq!(
        plan.replacement,
        "(cl-user:with-accessors ((used used-slot))\n  value\n  (list used))"
    );
}
