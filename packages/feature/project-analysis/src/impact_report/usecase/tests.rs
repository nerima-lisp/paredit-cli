use std::path::PathBuf;

use proptest::prelude::*;

use crate::application::usecase::signature_report::SignatureCallStatus;
use crate::domain::dialect::Dialect;
use crate::domain::refactor_plan::RefactorPlanSummary;
use crate::domain::sexpr::{SymbolName, SyntaxTree};

use super::*;

fn source(path: &str, input: &str) -> ImpactReportSource {
    ImpactReportSource {
        path: PathBuf::from(path),
        dialect: Dialect::CommonLisp,
        tree: SyntaxTree::parse(input).unwrap(),
    }
}

#[test]
fn builds_cross_file_impact_report() {
    let symbol = SymbolName::new("target").unwrap();
    let reports = build_impact_reports(
        vec![
            source(
                "src/a.lisp",
                "(in-package :demo)\n(defun target (x y) (+ x y))\n(defun holder () (list target))",
            ),
            source(
                "src/b.lisp",
                "(defun caller () (target 1) (target 1 2 3))\n(defun target-wrapper (z) (target z z))",
            ),
        ],
        &symbol,
    )
    .unwrap();

    let summary = summarize_impact_reports(&reports);
    let by_status = impact_status_counts(&reports);

    assert_eq!(summary.file_count, 2);
    assert_eq!(summary.definition_count, 1);
    assert_eq!(summary.call_count, 3);
    assert!(summary.inbound_edge_count >= 3);
    assert!(summary.non_call_reference_count >= 1);
    assert_eq!(by_status.get(&SignatureCallStatus::Exact), Some(&1));
    assert_eq!(
        by_status.get(&SignatureCallStatus::MissingArguments),
        Some(&1)
    );
    assert_eq!(
        by_status.get(&SignatureCallStatus::ExtraArguments),
        Some(&1)
    );
}

#[test]
fn evaluates_policy_failures() {
    let summary = RefactorPlanSummary {
        file_count: 1,
        definition_count: 0,
        reference_count: 1,
        call_count: 0,
        inbound_edge_count: 0,
        outbound_edge_count: 0,
        non_call_reference_count: 1,
        signature_mismatch_count: 0,
        safe_to_automate: false,
    };

    let policy = evaluate_impact_report_policy(
        ImpactReportPolicyOptions::new(Some(ImpactRiskLevel::Warning), Some(1), Some(2), Some(1))
            .unwrap(),
        &summary,
        ImpactRiskLevel::Error,
    );

    assert!(!policy.passed);
    assert_eq!(policy.violations.len(), 4);
}

#[test]
fn counts_common_lisp_setf_place_calls_and_edges() {
    let symbol = SymbolName::new("accessor").unwrap();
    let reports = build_impact_reports(
        vec![source(
            "src/core.lisp",
            "(define-setf-expander accessor (place) (values nil nil '(store) '(writer store) '(reader place)))\n(defun render (item) (setf (accessor item) 1) accessor)\n(defun wrapper (item) (setf (accessor item) 2))",
        )],
        &symbol,
    )
    .unwrap();

    let summary = summarize_impact_reports(&reports);

    assert_eq!(summary.definition_count, 1);
    assert_eq!(summary.call_count, 2);
    assert_eq!(summary.inbound_edge_count, 2);
    assert_eq!(summary.non_call_reference_count, 1);
    assert_eq!(reports[0].calls.len(), 2);
    assert!(
        reports[0]
            .calls
            .iter()
            .all(|call| call.status == SignatureCallStatus::Exact)
    );
}

#[test]
fn tracks_common_lisp_symbol_macro_without_signature_mismatch() {
    let symbol = SymbolName::new("current-user").unwrap();
    let reports = build_impact_reports(
        vec![source(
            "src/session.lisp",
            "(define-symbol-macro current-user (slot-value *session* 'user))\n(defun render () current-user)",
        )],
        &symbol,
    )
    .unwrap();

    let summary = summarize_impact_reports(&reports);
    let by_status = impact_status_counts(&reports);

    assert_eq!(summary.definition_count, 1);
    assert_eq!(summary.reference_count, 2);
    assert_eq!(summary.call_count, 0);
    assert_eq!(summary.inbound_edge_count, 0);
    assert_eq!(summary.non_call_reference_count, 1);
    assert!(by_status.is_empty());
    assert_eq!(reports[0].definitions.len(), 1);
    assert_eq!(
        reports[0].definitions[0].category,
        crate::domain::definition::DefinitionCategory::Variable
    );
    assert_eq!(reports[0].definitions[0].parameter_count, None);
}

#[test]
fn excludes_symbol_macrolet_binding_names_and_shadowed_body_references_from_impact() {
    let symbol = SymbolName::new("helper").unwrap();
    let reports = build_impact_reports(
        vec![source(
            "src/core.lisp",
            "(in-package #:app)\n(defun helper () 1)\n(defun caller ()\n  (cl:symbol-macrolet ((helper (helper)))\n    helper))",
        )],
        &symbol,
    )
    .unwrap();

    let summary = summarize_impact_reports(&reports);

    assert_eq!(summary.definition_count, 1);
    assert_eq!(summary.reference_count, 1);
    assert_eq!(summary.call_count, 1);
    assert_eq!(summary.inbound_edge_count, 1);
    assert_eq!(summary.non_call_reference_count, 0);
    assert_eq!(reports[0].calls.len(), 1);
    assert_eq!(reports[0].calls[0].call.head, "helper");
}

#[test]
fn matches_unqualified_references_and_calls_to_package_qualified_common_lisp_definition() {
    let symbol = SymbolName::new("target").unwrap();
    let reports = build_impact_reports(
        vec![source(
            "src/core.lisp",
            "(defun cl-user:target (x) target)\n(defun caller () (target 1) target)",
        )],
        &symbol,
    )
    .unwrap();

    let summary = summarize_impact_reports(&reports);
    let by_status = impact_status_counts(&reports);

    assert_eq!(summary.definition_count, 1);
    assert_eq!(summary.call_count, 1);
    assert_eq!(summary.inbound_edge_count, 1);
    assert_eq!(summary.non_call_reference_count, 1);
    assert_eq!(by_status.get(&SignatureCallStatus::Exact), Some(&1));
    assert_eq!(reports[0].definitions.len(), 1);
    assert_eq!(
        reports[0].definitions[0].name.as_deref(),
        Some("cl-user:target")
    );
    assert_eq!(reports[0].calls.len(), 1);
    assert_eq!(reports[0].calls[0].call.head, "target");
    assert_eq!(reports[0].calls[0].status, SignatureCallStatus::Exact);
}

proptest! {
    #[test]
    fn classifies_generated_call_arity(parameter_count in 0usize..6, argument_count in 0usize..6) {
        let symbol = SymbolName::new("target").unwrap();
        let params = (0..parameter_count)
            .map(|index| format!("p{index}"))
            .collect::<Vec<_>>()
            .join(" ");
        let args = (0..argument_count)
            .map(|index| index.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        let input = format!("(defun target ({params}) target)\n(defun caller () (target {args}))");
        let reports = build_impact_reports(vec![source("generated.lisp", &input)], &symbol).unwrap();
        let status = reports[0].calls[0].status;
        let expected = match argument_count.cmp(&parameter_count) {
            std::cmp::Ordering::Less => SignatureCallStatus::MissingArguments,
            std::cmp::Ordering::Equal => SignatureCallStatus::Exact,
            std::cmp::Ordering::Greater => SignatureCallStatus::ExtraArguments,
        };

        prop_assert_eq!(status, expected);
    }
}
