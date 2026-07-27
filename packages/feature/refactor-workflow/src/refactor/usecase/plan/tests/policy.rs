use super::fixtures::{gates, summary};
use super::*;

#[test]
fn policy_fails_on_blocking_gates_and_required_counts() {
    let gates = vec![RefactorPlanGate {
        level: RefactorRiskLevel::Error,
        code: "ambiguous-definition",
        message: "ambiguous definition".to_owned(),
        count: 2,
        blocks_automation: true,
    }];
    let policy = evaluate_refactor_plan_policy(
        RefactorPlanPolicyOptions::new(true, Some(2), Some(4)).expect("valid policy options"),
        &summary(),
        &gates,
    );

    assert!(!policy.passed);
    assert_eq!(policy.blocking_gate_count, 1);
    assert_eq!(policy.violations.len(), 3);
    assert!(
        policy
            .violations
            .iter()
            .any(|violation| { violation == "--fail-on-blocking-gate found 1 blocking gate(s)" })
    );
    assert!(
        policy
            .violations
            .iter()
            .any(|violation| { violation == "--require-definitions expected at least 2, found 1" })
    );
    assert!(
        policy
            .violations
            .iter()
            .any(|violation| { violation == "--require-references expected at least 4, found 3" })
    );
}

#[test]
fn automation_decision_prefers_policy_failure_over_manual_review() {
    let steps = refactor_plan_steps(
        RefactorOperation::Rename,
        "render-pane",
        &[std::path::PathBuf::from("core.lisp")],
        RefactorPlanTargetKind::Callable,
        &gates(1, 0),
    );
    let policy = RefactorPlanPolicy {
        fail_on_blocking_gate: true,
        require_definitions: None,
        require_references: None,
        blocking_gate_count: 1,
        definition_count: 1,
        reference_count: 3,
        passed: false,
        violations: vec!["--fail-on-blocking-gate found 1 blocking gate(s)".to_owned()],
    };

    let decision = refactor_plan_automation_decision(&policy, &steps);

    assert_eq!(
        decision.status(),
        RefactorPlanAutomationStatus::PolicyFailed
    );
    assert_eq!(decision.next_action(), "resolve-policy-violations");
    assert!(!decision.safe_to_automate());
    assert!(!decision.policy_passed());
    assert_eq!(decision.blocking_gate_count(), 1);
    assert_eq!(
        decision.steps(),
        [
            RefactorPlanAutomationStep {
                name: "plan-policy",
                status: RefactorPlanAutomationStepStatus::Failed,
            },
            RefactorPlanAutomationStep {
                name: "manual-review-gates",
                status: RefactorPlanAutomationStepStatus::Skipped,
            },
            RefactorPlanAutomationStep {
                name: "apply-plan",
                status: RefactorPlanAutomationStepStatus::Skipped,
            },
        ]
    );
}

#[test]
fn automation_decision_tracks_ready_and_manual_review_states() {
    let ready_policy = RefactorPlanPolicy {
        fail_on_blocking_gate: false,
        require_definitions: None,
        require_references: None,
        blocking_gate_count: 0,
        definition_count: 1,
        reference_count: 3,
        passed: true,
        violations: Vec::new(),
    };
    let ready_steps = refactor_plan_steps(
        RefactorOperation::Rename,
        "render-pane",
        &[std::path::PathBuf::from("core.lisp")],
        RefactorPlanTargetKind::Callable,
        &[],
    );

    let ready = refactor_plan_automation_decision(&ready_policy, &ready_steps);

    assert_eq!(ready.status(), RefactorPlanAutomationStatus::Ready);
    assert_eq!(ready.next_action(), "apply-rename");
    assert!(ready.safe_to_automate());
    assert!(ready.policy_passed());
    assert_eq!(
        ready.steps(),
        [
            RefactorPlanAutomationStep {
                name: "plan-policy",
                status: RefactorPlanAutomationStepStatus::Passed,
            },
            RefactorPlanAutomationStep {
                name: "manual-review-gates",
                status: RefactorPlanAutomationStepStatus::Passed,
            },
            RefactorPlanAutomationStep {
                name: "apply-plan",
                status: RefactorPlanAutomationStepStatus::Scheduled,
            },
        ]
    );

    let manual_policy = RefactorPlanPolicy {
        blocking_gate_count: 1,
        ..ready_policy
    };
    let manual_steps = refactor_plan_steps(
        RefactorOperation::Rename,
        "render-pane",
        &[std::path::PathBuf::from("core.lisp")],
        RefactorPlanTargetKind::Callable,
        &gates(1, 0),
    );

    let manual = refactor_plan_automation_decision(&manual_policy, &manual_steps);

    assert_eq!(manual.status(), RefactorPlanAutomationStatus::ManualReview);
    assert_eq!(manual.next_action(), "review-rename-scope");
    assert!(!manual.safe_to_automate());
    assert!(manual.policy_passed());
    assert_eq!(
        manual.steps(),
        [
            RefactorPlanAutomationStep {
                name: "plan-policy",
                status: RefactorPlanAutomationStepStatus::Passed,
            },
            RefactorPlanAutomationStep {
                name: "manual-review-gates",
                status: RefactorPlanAutomationStepStatus::Scheduled,
            },
            RefactorPlanAutomationStep {
                name: "apply-plan",
                status: RefactorPlanAutomationStepStatus::Skipped,
            },
        ]
    );

    let symbol_macro_policy = RefactorPlanPolicy {
        fail_on_blocking_gate: false,
        require_definitions: None,
        require_references: None,
        blocking_gate_count: 0,
        definition_count: 1,
        reference_count: 3,
        passed: true,
        violations: Vec::new(),
    };
    for target_kind in [
        RefactorPlanTargetKind::Macro,
        RefactorPlanTargetKind::CompilerMacro,
        RefactorPlanTargetKind::SetfExpander,
        RefactorPlanTargetKind::SymbolMacro,
    ] {
        let steps = refactor_plan_steps(
            RefactorOperation::Signature,
            "current-user",
            &[std::path::PathBuf::from("core.lisp")],
            target_kind,
            &[],
        );
        let manual = refactor_plan_automation_decision(&symbol_macro_policy, &steps);

        assert_eq!(manual.status(), RefactorPlanAutomationStatus::ManualReview);
        assert_eq!(manual.next_action(), "review-signature-scope");
        assert!(!manual.safe_to_automate());
        assert!(manual.policy_passed());
        assert_eq!(manual.blocking_gate_count(), 0);
        assert_eq!(
            manual.steps(),
            [
                RefactorPlanAutomationStep {
                    name: "plan-policy",
                    status: RefactorPlanAutomationStepStatus::Passed,
                },
                RefactorPlanAutomationStep {
                    name: "manual-review-gates",
                    status: RefactorPlanAutomationStepStatus::Scheduled,
                },
                RefactorPlanAutomationStep {
                    name: "apply-plan",
                    status: RefactorPlanAutomationStepStatus::Skipped,
                },
            ]
        );
    }
}
