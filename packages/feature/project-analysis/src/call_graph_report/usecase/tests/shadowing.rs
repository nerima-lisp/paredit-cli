use super::*;

#[test]
fn skips_common_lisp_local_callable_edges_to_shadowed_global_definitions() {
    let report = build_call_graph_report(
        vec![source(
            "(defun helper (x y) y)\n(defun main () (flet ((helper (x) (target x))) (helper 1)))\n(defun target (x) x)",
        )],
        false,
        None,
    )
    .unwrap();
    let edges = &report.files[0].edges;

    assert!(
        !edges
            .iter()
            .any(|edge| edge.caller.as_deref() == Some("main") && edge.callee == "helper")
    );
    assert!(
        edges
            .iter()
            .any(|edge| edge.caller.as_deref() == Some("main") && edge.callee == "target")
    );
}

#[test]
fn skips_common_lisp_macrolet_shadowed_global_definition_edges() {
    let report = build_call_graph_report(
        vec![source(
            "(defun helper (x) (+ x 1))\n(defun render () (macrolet ((helper (x) (helper x))) (helper 2)) (helper 3))",
        )],
        false,
        None,
    )
    .unwrap();
    assert_shadowed_macrolet_edges(&report);
}

#[test]
fn skips_common_lisp_cl_user_macrolet_shadowed_global_definition_edges() {
    let report = build_call_graph_report(
        vec![source(
            "(defun helper (x) (+ x 1))\n(defun render () (cl-user:macrolet ((helper (x) (helper x))) (helper 2)) (helper 3))",
        )],
        false,
        None,
    )
    .unwrap();
    assert_shadowed_macrolet_edges(&report);
}

#[test]
fn skips_common_lisp_cl_user_flet_local_callable_edges_to_shadowed_global_definitions() {
    let report = build_call_graph_report(
        vec![source(
            "(defun helper (x y) y)\n(defun main () (cl-user:flet ((helper (x) (target x))) (helper 1)))\n(defun target (x) x)",
        )],
        false,
        None,
    )
    .unwrap();
    assert_no_shadowed_helper_edge(&report, "main");
    assert_has_target_edge(&report, "main");
}

#[test]
fn skips_common_lisp_cl_flet_local_callable_edges_to_shadowed_global_definitions() {
    let report = build_call_graph_report(
        vec![source(
            "(defun helper (x y) y)\n(defun main () (cl:flet ((helper (x) (target x))) (helper 1)))\n(defun target (x) x)",
        )],
        false,
        None,
    )
    .unwrap();
    assert_no_shadowed_helper_edge(&report, "main");
    assert_has_target_edge(&report, "main");
}

#[test]
fn skips_common_lisp_cl_user_labels_local_callable_edges_to_shadowed_global_definitions() {
    let report = build_call_graph_report(
        vec![source(
            "(defun helper (x y) y)\n(defun main () (cl-user:labels ((helper (x) (target x))) (helper 1)))\n(defun target (x) x)",
        )],
        false,
        None,
    )
    .unwrap();
    assert_no_shadowed_helper_edge(&report, "main");
    assert_has_target_edge(&report, "main");
}

#[test]
fn skips_common_lisp_cl_labels_local_callable_edges_to_shadowed_global_definitions() {
    let report = build_call_graph_report(
        vec![source(
            "(defun helper (x y) y)\n(defun main () (cl:labels ((helper (x) (target x))) (helper 1)))\n(defun target (x) x)",
        )],
        false,
        None,
    )
    .unwrap();
    assert_no_shadowed_helper_edge(&report, "main");
    assert_has_target_edge(&report, "main");
}

#[test]
fn skips_emacs_lisp_cl_macrolet_shadowed_global_definition_edges() {
    let report = build_call_graph_report(
        vec![source_with_dialect(
            "(defun helper (x) (+ x 1))\n(defun render () (cl-macrolet ((helper (x) (helper x))) (helper 2)) (helper 3))",
            "sample.el",
            Dialect::EmacsLisp,
        )],
        false,
        None,
    )
    .unwrap();
    assert_shadowed_macrolet_edges(&report);
}

#[test]
fn skips_common_lisp_cl_macrolet_shadowed_global_definition_edges() {
    let report = build_call_graph_report(
        vec![source(
            "(defun helper (x) (+ x 1))\n(defun render () (cl:macrolet ((helper (x) (helper x))) (helper 2)) (helper 3))",
        )],
        false,
        None,
    )
    .unwrap();
    assert_shadowed_macrolet_edges(&report);
}

#[test]
fn skips_common_lisp_compiler_macrolet_shadowed_global_definition_edges() {
    let report = build_call_graph_report(
        vec![source(
            "(defun helper (x) (+ x 1))\n(defun render () (compiler-macrolet ((helper (x) (helper x))) (helper 2)) (helper 3))",
        )],
        false,
        None,
    )
    .unwrap();
    assert_shadowed_macrolet_edges(&report);
}

#[test]
fn skips_common_lisp_cl_compiler_macrolet_shadowed_global_definition_edges() {
    let report = build_call_graph_report(
        vec![source(
            "(defun helper (x) (+ x 1))\n(defun render () (cl:compiler-macrolet ((helper (x) (helper x))) (helper 2)) (helper 3))",
        )],
        false,
        None,
    )
    .unwrap();
    assert_shadowed_macrolet_edges(&report);
}

#[test]
fn skips_common_lisp_cl_user_compiler_macrolet_shadowed_global_definition_edges() {
    let report = build_call_graph_report(
        vec![source(
            "(defun helper (x) (+ x 1))\n(defun render () (cl-user:compiler-macrolet ((helper (x) (helper x))) (helper 2)) (helper 3))",
        )],
        false,
        None,
    )
    .unwrap();
    assert_shadowed_macrolet_edges(&report);
}

fn assert_no_shadowed_helper_edge(report: &CallGraphReport, caller: &str) {
    assert!(
        !report.files[0]
            .edges
            .iter()
            .any(|edge| { edge.caller.as_deref() == Some(caller) && edge.callee == "helper" })
    );
}

fn assert_has_target_edge(report: &CallGraphReport, caller: &str) {
    assert!(
        report.files[0]
            .edges
            .iter()
            .any(|edge| { edge.caller.as_deref() == Some(caller) && edge.callee == "target" })
    );
}

fn assert_shadowed_macrolet_edges(report: &CallGraphReport) {
    let edges = &report.files[0].edges;

    assert_eq!(edges.len(), 2);
    assert!(
        edges
            .iter()
            .all(|edge| edge.caller.as_deref() == Some("render"))
    );
    assert!(edges.iter().all(|edge| edge.callee == "helper"));
    assert!(edges.iter().any(|edge| edge.path == "1.3.1.0.2"));
    assert!(edges.iter().any(|edge| edge.path == "1.4"));
}
