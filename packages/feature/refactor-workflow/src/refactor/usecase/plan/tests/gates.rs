use super::fixtures::summary;
use super::*;

#[test]
fn rename_plan_blocks_ambiguous_definitions() {
    let gates = refactor_plan_gates(
        RefactorOperation::Rename,
        RefactorPlanTargetKind::Callable,
        &summary(),
        vec![RawRefactorRisk {
            level: RefactorRiskLevel::Warning,
            code: "ambiguous-definition",
            message: "multiple definitions".to_owned(),
            count: 2,
        }],
    );

    assert!(
        gates
            .iter()
            .any(|gate| gate.code == "ambiguous-definition" && gate.blocks_automation)
    );
    let policy = evaluate_refactor_plan_policy(
        RefactorPlanPolicyOptions::new(true, None, None).expect("valid policy options"),
        &summary(),
        &gates,
    );
    assert!(!policy.passed);
}

#[test]
fn rename_plan_does_not_block_signature_mismatch_for_macro_targets() {
    let gates = refactor_plan_gates(
        RefactorOperation::Rename,
        RefactorPlanTargetKind::Macro,
        &summary(),
        vec![RawRefactorRisk {
            level: RefactorRiskLevel::Warning,
            code: "signature-mismatch",
            message: "macro-like definition".to_owned(),
            count: 1,
        }],
    );

    assert!(
        gates
            .iter()
            .any(|gate| gate.code == "signature-mismatch" && !gate.blocks_automation)
    );
}

#[test]
fn rename_plan_does_not_block_signature_mismatch_for_macro_like_targets() {
    for target_kind in [
        RefactorPlanTargetKind::CompilerMacro,
        RefactorPlanTargetKind::SetfExpander,
        RefactorPlanTargetKind::SymbolMacro,
    ] {
        let gates = refactor_plan_gates(
            RefactorOperation::Rename,
            target_kind,
            &summary(),
            vec![RawRefactorRisk {
                level: RefactorRiskLevel::Warning,
                code: "signature-mismatch",
                message: "macro-like definition".to_owned(),
                count: 1,
            }],
        );

        assert!(
            gates
                .iter()
                .any(|gate| gate.code == "signature-mismatch" && !gate.blocks_automation)
        );
    }
}

#[test]
fn symbol_macro_target_kind_is_treated_as_macro_like_for_signature_compatibility() {
    assert!(RefactorPlanTargetKind::SymbolMacro.is_macro_like());
    assert!(RefactorPlanTargetKind::SymbolMacro.skips_signature_compatibility());
}

#[test]
fn symbol_macro_target_kind_skips_call_coverage_for_non_signature_operations() {
    assert!(!RefactorPlanTargetKind::SymbolMacro.requires_call_coverage(RefactorOperation::Rename));
    assert!(!RefactorPlanTargetKind::SymbolMacro.requires_call_coverage(RefactorOperation::Move));
    assert!(RefactorPlanTargetKind::SymbolMacro.requires_call_coverage(RefactorOperation::Remove));
    assert!(
        !RefactorPlanTargetKind::SymbolMacro.requires_call_coverage(RefactorOperation::Signature)
    );
}

#[test]
fn rename_plan_still_blocks_signature_mismatch_for_callable_targets() {
    let gates = refactor_plan_gates(
        RefactorOperation::Rename,
        RefactorPlanTargetKind::Callable,
        &summary(),
        vec![RawRefactorRisk {
            level: RefactorRiskLevel::Warning,
            code: "signature-mismatch",
            message: "callable signature changed".to_owned(),
            count: 1,
        }],
    );

    assert!(
        gates
            .iter()
            .any(|gate| gate.code == "signature-mismatch" && gate.blocks_automation)
    );
}

#[test]
fn risk_summary_counts_gate_occurrences_by_level_and_blocking_status() {
    let gates = vec![
        RefactorPlanGate {
            level: RefactorRiskLevel::Info,
            code: "context",
            message: "context only".to_owned(),
            count: 2,
            blocks_automation: false,
        },
        RefactorPlanGate {
            level: RefactorRiskLevel::Warning,
            code: "manual-review",
            message: "needs review".to_owned(),
            count: 3,
            blocks_automation: true,
        },
        RefactorPlanGate {
            level: RefactorRiskLevel::Error,
            code: "parse-error",
            message: "cannot parse".to_owned(),
            count: 1,
            blocks_automation: true,
        },
    ];

    let summary = RefactorPlanRiskSummary::from_gates(&gates);

    assert_eq!(summary.highest_level, Some(RefactorRiskLevel::Error));
    assert_eq!(summary.info_count, 2);
    assert_eq!(summary.warning_count, 3);
    assert_eq!(summary.error_count, 1);
    assert_eq!(summary.blocking_count, 4);
    assert_eq!(summary.advisory_count, 2);
}
