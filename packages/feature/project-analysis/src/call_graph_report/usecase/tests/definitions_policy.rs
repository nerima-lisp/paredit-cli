use super::*;

#[test]
fn counts_common_lisp_macro_and_method_lambda_lists() {
    let report = build_call_graph_report(
        vec![source(
            "(define-compiler-macro fast-add (x y) (helper x y))\n(defmethod render :around ((node widget) stream) (draw node stream))\n(defun helper (x y) y)\n(defun draw (node stream) stream)",
        )],
        false,
        None,
    )
    .unwrap();

    let definitions = &report.files[0].definitions;
    let fast_add = definitions
        .iter()
        .find(|definition| definition.name.as_deref() == Some("fast-add"))
        .expect("compiler macro definition");
    let render = definitions
        .iter()
        .find(|definition| definition.name.as_deref() == Some("render"))
        .expect("method definition");

    assert_eq!(fast_add.parameter_count, 2);
    assert_eq!(render.parameter_count, 2);
    assert!(
        !report.files[0]
            .edges
            .iter()
            .any(|edge| edge.caller.as_deref() == Some("render") && edge.callee == "node")
    );
    assert!(
        report.files[0]
            .edges
            .iter()
            .any(|edge| edge.caller.as_deref() == Some("render") && edge.callee == "draw")
    );
}

#[test]
fn policy_counts_inbound_edges_without_self_recursive_calls() {
    let symbol = SymbolName::new("target").unwrap();
    let report = build_call_graph_report(
        vec![source(
            "(defun target (x) (target x))\n(defun caller (y) (target y))",
        )],
        false,
        Some(&symbol),
    )
    .unwrap();

    let policy = evaluate_call_graph_policy(
        &report.files,
        Some(&symbol),
        CallGraphPolicyOptions::new(true, Some(2), Some(2)).unwrap(),
    );

    assert_eq!(policy.edge_count, 2);
    assert_eq!(policy.internal_edge_count, 2);
    assert_eq!(
        policy.inbound_callers.iter().cloned().collect::<Vec<_>>(),
        vec!["caller".to_string()]
    );
}

#[test]
fn reports_common_lisp_setf_place_edges_for_setf_callables() {
    let report = build_call_graph_report(
        vec![source(
            "(define-setf-expander accessor (place) (values nil nil '(store) '(writer store) '(reader place)))\n(defun render (item) (setf (accessor item) 1) accessor)\n(defun wrapper (item) (setf (accessor item) 2))",
        )],
        false,
        None,
    )
    .unwrap();
    let edges = &report.files[0].edges;

    assert_eq!(edges.len(), 2);
    assert!(
        edges
            .iter()
            .all(|edge| edge.callee == "accessor" && edge.internal)
    );
    assert!(
        edges
            .iter()
            .any(|edge| edge.caller.as_deref() == Some("render") && edge.argument_count == 1)
    );
    assert!(
        edges
            .iter()
            .any(|edge| edge.caller.as_deref() == Some("wrapper") && edge.argument_count == 1)
    );
}

#[test]
fn records_common_lisp_symbol_macro_as_a_graph_node_without_edges() {
    let report = build_call_graph_report(
        vec![source(
            "(define-symbol-macro current-user (slot-value *session* 'user))\n(defun render () current-user)",
        )],
        false,
        None,
    )
    .unwrap();

    assert_eq!(report.nodes_by_name.len(), 2);
    assert!(report.nodes_by_name.contains_key("current-user"));
    assert!(report.nodes_by_name.contains_key("render"));
    assert_eq!(report.files[0].definitions.len(), 2);
    assert_eq!(report.files[0].edges.len(), 0);
    assert_eq!(
        report.files[0]
            .definitions
            .iter()
            .find(|definition| definition.name.as_deref() == Some("current-user"))
            .map(|definition| definition.category),
        Some(DefinitionCategory::Variable)
    );
}

#[test]
fn records_common_lisp_symbol_macrolet_expansion_and_body_edges_without_binding_name_edges() {
    let report = build_call_graph_report(
        vec![source(
            "(defun helper (x) (+ x 10))\n(defun target (x) x)\n(defun render () (symbol-macrolet ((helper (target 1))) (list helper (target 2))))",
        )],
        false,
        None,
    )
    .unwrap();
    let edges = &report.files[0].edges;

    assert_eq!(edges.len(), 2);
    assert!(
        edges
            .iter()
            .all(|edge| edge.caller.as_deref() == Some("render"))
    );
    assert!(edges.iter().all(|edge| edge.callee == "target"));
    assert!(!edges.iter().any(|edge| edge.callee == "helper"));
    assert!(edges.iter().any(|edge| edge.argument_count == 1));
}

#[test]
fn skips_common_lisp_locally_declare_forms_in_call_graph_edges() {
    let report = build_call_graph_report(
        vec![source(
            "(defun main () (locally (declare (special target)) (target 1)))\n(defun target (x) x)",
        )],
        false,
        None,
    )
    .unwrap();
    let edges = &report.files[0].edges;

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].caller.as_deref(), Some("main"));
    assert_eq!(edges[0].callee, "target");
    assert!(edges[0].internal);
}
