use super::*;

#[test]
fn dolist_iteration_variable_shadows_body_and_result() {
    let input = "(list value (dolist (value values value) value) value)";

    assert_eq!(reference_texts(input, "value"), vec!["value", "value"]);
}

#[test]
fn dotimes_iteration_variable_shadows_body_and_result() {
    let input = "(list limit (dotimes (limit limit limit) limit) limit)";

    assert_eq!(
        reference_texts(input, "limit"),
        vec!["limit", "limit", "limit"]
    );
}

#[test]
fn do_variables_shadow_steps_end_clause_and_body_but_not_inits() {
    let input = "(list i (do ((i i (1+ i)) (sum i (+ sum i))) ((>= i limit) i) i) i)";

    assert_eq!(reference_texts(input, "i"), vec!["i", "i", "i", "i"]);
}

#[test]
fn do_star_variables_shadow_later_inits_and_body() {
    let input = "(list i (do* ((i i (1+ i)) (sum i (+ sum i))) ((>= sum limit) i) sum) i)";

    assert_eq!(reference_texts(input, "i"), vec!["i", "i", "i"]);
}

#[test]
fn prog_variables_shadow_body_but_not_inits() {
    let input = "(list value (prog ((value value) (copy value)) value (return value)) value)";

    assert_eq!(
        reference_texts(input, "value"),
        vec!["value", "value", "value", "value"]
    );
}

#[test]
fn prog_star_variables_shadow_later_inits_and_body() {
    let input = "(list value (prog* ((value value) (copy value)) (return value)) value)";

    assert_eq!(
        reference_texts(input, "value"),
        vec!["value", "value", "value"]
    );
}
