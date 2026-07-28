use super::*;

#[test]
fn destructuring_bind_checks_value_before_body_shadowing() {
    let input = "(list x (destructuring-bind (x) x x) x)";

    assert_eq!(reference_texts(input, "x"), vec!["x", "x", "x"]);
}

#[test]
fn multiple_value_bind_checks_value_before_body_shadowing() {
    let input = "(list x (multiple-value-bind (x) x x) x)";

    assert_eq!(reference_texts(input, "x"), vec!["x", "x", "x"]);
}

#[test]
fn qualified_common_lisp_binding_heads_check_value_before_body_shadowing() {
    let input = "(list x (cl:destructuring-bind (x) x x) (cl-user:multiple-value-bind (x) x x) x)";

    assert_eq!(reference_texts(input, "x"), vec!["x", "x", "x", "x"]);
}
