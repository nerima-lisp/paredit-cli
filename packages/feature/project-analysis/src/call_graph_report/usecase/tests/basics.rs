use super::*;

#[test]
fn builds_internal_edges_across_definitions() {
    let report = build_call_graph_report(
        vec![source("(defun helper (x) x)\n(defun main (y) (helper y))")],
        false,
        None,
    )
    .unwrap();

    assert_eq!(report.nodes_by_name.len(), 2);
    assert!(report.nodes_by_name.contains_key("helper"));
    assert!(report.nodes_by_name.contains_key("main"));
    assert_eq!(report.files[0].definitions.len(), 2);
    assert_eq!(report.files[0].edges.len(), 1);
    assert_eq!(report.files[0].edges[0].caller.as_deref(), Some("main"));
    assert_eq!(report.files[0].edges[0].callee, "helper");
    assert!(report.files[0].edges[0].internal);
    assert!(
        report.files[0].edges[0]
            .callee_categories
            .contains(&DefinitionCategory::Function)
    );
}

#[test]
fn can_include_external_edges_and_filter_by_symbol() {
    let symbol = SymbolName::new("missing").unwrap();
    let report = build_call_graph_report(
        vec![source(
            "(defun main (y) (missing y) (helper y))\n(defun helper (x) x)",
        )],
        true,
        Some(&symbol),
    )
    .unwrap();

    assert_eq!(report.files[0].edges.len(), 1);
    assert_eq!(report.files[0].edges[0].caller.as_deref(), Some("main"));
    assert_eq!(report.files[0].edges[0].callee, "missing");
    assert!(!report.files[0].edges[0].internal);
}

#[test]
fn resolves_unqualified_call_edge_to_package_qualified_common_lisp_definition() {
    let symbol = SymbolName::new("target").unwrap();
    let report = build_call_graph_report(
        vec![source(
            "(defun cl-user:target (x) x)\n(defun caller () (target 1))",
        )],
        false,
        Some(&symbol),
    )
    .unwrap();

    assert_eq!(report.nodes_by_name.len(), 2);
    assert_eq!(report.files[0].edges.len(), 1);
    assert_eq!(report.files[0].edges[0].caller.as_deref(), Some("caller"));
    assert_eq!(report.files[0].edges[0].callee, "target");
    assert!(report.files[0].edges[0].internal);
    assert!(
        report.files[0].edges[0]
            .callee_categories
            .contains(&DefinitionCategory::Function)
    );
}

#[test]
fn in_package_forms_do_not_count_as_package_definitions() {
    let report = build_call_graph_report(
        vec![source(
            "(defpackage :demo (:use :cl))\n(in-package :demo)\n(defun helper (x) x)\n(in-package :demo)\n",
        )],
        false,
        None,
    )
    .unwrap();

    // The package is defined once by defpackage; the two in-package forms are
    // references, not definitions.
    let package_node = report
        .nodes_by_name
        .get(":demo")
        .expect("package node present");
    assert_eq!(package_node.definition_count, 1);
    // Only defpackage and helper are definitions; in-package is excluded.
    assert_eq!(report.files[0].definitions.len(), 2);
}

proptest! {
    #[test]
    fn pbt_internal_edges_preserve_generated_callee_and_arity(
        caller in symbol_strategy(),
        callee in symbol_strategy(),
        arity in 0usize..6,
    ) {
        prop_assume!(caller != callee);
        let params = (0..arity)
            .map(|index| format!("x{index}"))
            .collect::<Vec<_>>()
            .join(" ");
        let args = (0..arity)
            .map(|index| format!("x{index}"))
            .collect::<Vec<_>>()
            .join(" ");
        let input = format!(
            "(defun {callee} ({params}) {callee})\n(defun {caller} ({params}) ({callee} {args}))"
        );

        let report = build_call_graph_report(vec![source(&input)], false, None).unwrap();
        let matching_edges = report.files[0]
            .edges
            .iter()
            .filter(|edge| edge.caller.as_deref() == Some(caller.as_str()) && edge.callee == callee)
            .collect::<Vec<_>>();

        prop_assert_eq!(matching_edges.len(), 1);
        prop_assert_eq!(matching_edges[0].argument_count, arity);
        prop_assert!(matching_edges[0].internal);
    }
}
