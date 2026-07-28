use super::*;

#[test]
fn handler_case_clause_parameters_shadow_only_clause_body() {
    let input = "(list condition (handler-case (risky condition) (error (condition) condition) (:no-error (value) condition)) condition)";

    assert_eq!(
        reference_texts(input, "condition"),
        vec!["condition", "condition", "condition", "condition"]
    );
}

#[test]
fn restart_case_clause_parameters_shadow_only_clause_body() {
    let input = "(list condition (restart-case (risky condition) (retry (condition) condition) (skip () condition)) condition)";

    assert_eq!(
        reference_texts(input, "condition"),
        vec!["condition", "condition", "condition", "condition"]
    );
}

#[test]
fn handler_bind_function_lambda_parameters_shadow_only_handler_body() {
    let input = "(list condition (handler-bind ((error (lambda (condition) condition))) condition) condition)";

    assert_eq!(
        reference_texts(input, "condition"),
        vec!["condition", "condition", "condition"]
    );
}

#[test]
fn restart_bind_scans_restart_function_and_option_values_with_local_lambda_shadowing() {
    let input = "(list stream (restart-bind ((retry (lambda () stream) :report (lambda (stream) stream))) stream) stream)";

    assert_eq!(
        reference_texts(input, "stream"),
        vec!["stream", "stream", "stream", "stream"]
    );
}
