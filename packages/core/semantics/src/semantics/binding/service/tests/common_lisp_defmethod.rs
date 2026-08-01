//! `defmethod`/`cl-defmethod` binding, as a Common Lisp form in its own
//! right (not the Emacs Lisp `cl-defmethod` handling in `emacs_lisp.rs`,
//! which is a separate binder for a different dialect — see its own tests).
//!
//! A method's required parameters may each carry a specializer —
//! `(obj my-type)` — and only the parameter name is a binding; `my-type`
//! names the dispatch class and is never bound, matching how
//! `refactor rename-at` already treats the same shape.

use super::{binding_labels, build};

fn names(body: &str) -> Vec<String> {
    let table = build(body);
    binding_labels(&table, body)
        .into_iter()
        .map(|label| label.split('@').next().unwrap_or_default().to_owned())
        .collect()
}

/// The golden fixture (FR-009's acceptance criterion): a `defmethod` with a
/// specialized required parameter must bind exactly the same names as a
/// `defun` with the equivalent plain parameter list — the specializer
/// `my-type` must not appear anywhere in the binding table.
#[test]
fn defmethod_binds_the_same_names_as_the_equivalent_defun() {
    let defmethod = "(defmethod handle ((obj my-type) arg) (list obj arg))";
    let defun = "(defun handle (obj arg) (list obj arg))";

    assert_eq!(names(defmethod), vec!["obj", "arg"]);
    assert_eq!(names(defmethod), names(defun));
}

#[test]
fn defmethod_specializer_reference_is_never_bound() {
    let input = "(defmethod handle ((obj my-type) arg) (list obj arg))";
    assert!(!names(input).contains(&"my-type".to_owned()));
}

#[test]
fn defmethod_qualifier_does_not_break_parameter_detection() {
    for qualifier in [":before", ":after", ":around"] {
        let input = format!("(defmethod handle {qualifier} ((obj my-type) arg) (list obj arg))");
        assert_eq!(names(&input), vec!["obj", "arg"], "qualifier {qualifier}");
    }
}

/// A custom method combination qualifier (any keyword, not just the three
/// built-in ones) shifts the lambda list exactly the same way.
#[test]
fn defmethod_custom_qualifier_does_not_break_parameter_detection() {
    let input = "(defmethod handle :my-qualifier ((obj my-type) arg) (list obj arg))";
    assert_eq!(names(input), vec!["obj", "arg"]);
}

#[test]
fn defmethod_eql_specializer_does_not_crash_or_misbind() {
    let input = "(defmethod handle ((x (eql :foo))) (list x))";
    assert_eq!(names(input), vec!["x"]);
}

#[test]
fn defmethod_eql_specializer_on_a_symbol_does_not_bind_the_symbol() {
    // `(eql +some-constant+)`: the constant is read at method-definition
    // time, not bound as a parameter.
    let input = "(defmethod handle ((x (eql +some-constant+))) (list x))";
    assert_eq!(names(input), vec!["x"]);
}

#[test]
fn defmethod_optional_parameter_binds_past_the_specializer_section() {
    let input = "(defmethod handle ((obj my-type) &optional (flag t)) (list obj flag))";
    assert_eq!(names(input), vec!["obj", "flag"]);
}

#[test]
fn defmethod_rest_parameter_binds_past_the_specializer_section() {
    let input = "(defmethod handle ((obj my-type) &rest more) (list obj more))";
    assert_eq!(names(input), vec!["obj", "more"]);
}

#[test]
fn defmethod_key_parameter_binds_past_the_specializer_section() {
    let input = "(defmethod handle ((obj my-type) &key (mode :normal)) (list obj mode))";
    assert_eq!(names(input), vec!["obj", "mode"]);
}

#[test]
fn defmethod_optional_rest_and_key_parameters_all_bind_together() {
    let input = "(defmethod handle \
                  ((obj my-type) &optional (flag t) &rest more &key (mode :normal)) \
                  (list obj flag more mode))";
    assert_eq!(names(input), vec!["obj", "flag", "more", "mode"]);
}

#[test]
fn defmethod_unspecialized_required_parameter_still_binds() {
    // `arg` alone (no specializer list) is the common case and must keep
    // working exactly as it does today via the generic fallback.
    let input = "(defmethod handle (arg) arg)";
    assert_eq!(names(input), vec!["arg"]);
}

/// `cl-defmethod` used as a Common Lisp head (as opposed to the Emacs Lisp
/// `cl-defmethod` covered by `emacs_lisp.rs`'s own tests) goes through the
/// same dispatch and binds identically to `defmethod`.
#[test]
fn cl_defmethod_head_binds_like_defmethod() {
    let input = "(cl-defmethod handle ((obj my-type) arg) (list obj arg))";
    assert_eq!(names(input), vec!["obj", "arg"]);
}

/// Regression: this is an additive dispatch arm. `defun`'s own binding
/// behavior — including a plain parameter list with no specializers — must
/// be completely unaffected.
#[test]
fn defun_binding_is_unaffected_by_the_new_defmethod_arm() {
    let input = "(defun handle (obj arg) (list obj arg))";
    assert_eq!(names(input), vec!["obj", "arg"]);
}

/// Regression: `defgeneric` is not a method definition (`is_method_definition`
/// excludes it) and was never dispatched by `is_defun_like` either; its
/// binding behavior — no parameter scope opened — must stay exactly as it
/// was before this change.
#[test]
fn defgeneric_binding_is_unaffected_by_the_new_defmethod_arm() {
    let input = "(defgeneric handle (obj arg))";
    assert!(names(input).is_empty());
}
