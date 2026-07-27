use super::*;

fn call_heads(input: &str, dialect: Dialect) -> Vec<String> {
    build_call_report(&parse(input), dialect, None, false)
        .unwrap()
        .into_iter()
        .map(|call| call.head)
        .collect()
}

#[test]
fn skips_common_lisp_flet_local_callable_calls() {
    let heads = call_heads(
        "(defun main () (flet ((helper (x) (target x))) (helper 1) (target 2)))",
        Dialect::CommonLisp,
    );

    assert_eq!(heads, vec!["target", "target"]);
}

#[test]
fn skips_emacs_lisp_cl_flet_local_callable_calls() {
    let heads = call_heads(
        "(defun main () (cl-flet ((helper (x) (target x))) (helper 1) (target 2)))",
        Dialect::EmacsLisp,
    );

    assert_eq!(heads, vec!["target", "target"]);
}

#[test]
fn skips_common_lisp_cl_user_flet_local_callable_calls() {
    let heads = call_heads(
        "(defun main () (cl-user:flet ((helper (x) (target x))) (helper 1) (target 2)))",
        Dialect::CommonLisp,
    );

    assert_eq!(heads, vec!["target", "target"]);
}

#[test]
fn skips_common_lisp_cl_flet_local_callable_calls() {
    let heads = call_heads(
        "(defun main () (cl:flet ((helper (x) (target x))) (helper 1) (target 2)))",
        Dialect::CommonLisp,
    );

    assert_eq!(heads, vec!["target", "target"]);
}

#[test]
fn skips_common_lisp_cl_labels_local_callable_calls_in_definition_bodies() {
    let heads = call_heads(
        "(defun main () (cl:labels ((helper (x) (helper x) (target x))) (helper 1)))",
        Dialect::CommonLisp,
    );

    assert_eq!(heads, vec!["target"]);
}

#[test]
fn skips_common_lisp_labels_local_callable_calls_in_definition_bodies() {
    let heads = call_heads(
        "(defun main () (labels ((helper (x) (helper x) (target x))) (helper 1)))",
        Dialect::CommonLisp,
    );

    assert_eq!(heads, vec!["target"]);
}

#[test]
fn skips_emacs_lisp_cl_labels_local_callable_calls_in_definition_bodies() {
    let heads = call_heads(
        "(defun main () (cl-labels ((helper (x) (helper x) (target x))) (helper 1)))",
        Dialect::EmacsLisp,
    );

    assert_eq!(heads, vec!["target"]);
}

#[test]
fn skips_common_lisp_declare_forms_in_body_scans() {
    let heads = call_heads(
        "(defun main () (locally (declare (special target)) (target 1)))",
        Dialect::CommonLisp,
    );

    assert_eq!(heads, vec!["target"]);
}
