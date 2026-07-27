use super::*;
use proptest::prelude::*;

#[test]
fn write_plan_refuses_all_writes_when_any_output_does_not_parse() {
    let plan = build_refactor_write_plan(
        true,
        &[
            RefactorWriteCandidate {
                changed: true,
                output_parse_ok: true,
            },
            RefactorWriteCandidate {
                changed: true,
                output_parse_ok: false,
            },
        ],
    );

    assert!(plan.write_requested());
    assert!(!plan.write_allowed());
    assert!(plan.writable_indexes().is_empty());
    assert_eq!(
        plan.refusal(),
        Some(&RefactorWriteRefusal::UnparsableOutputs { count: 1 })
    );
    let refusal = plan.refusal().expect("write refusal");
    assert_eq!(refusal.label(), "unparsable-outputs");
    assert_eq!(refusal.reason(), "rewritten-output-did-not-parse");
    assert_eq!(refusal.next_action(), "inspect-preview-parse-errors");
}

#[test]
fn execute_decision_refuses_write_parse_failures_before_preflight() {
    let decision = build_refactor_execute_preflight_decision(RefactorExecutePreflightInputs::new(
        RefactorExecuteMode::Write,
        RefactorExecutePolicyResult::Passed,
        RefactorExecuteOutputParseResult::Unparsable,
    ));

    assert!(decision.write_parse_refused());
    assert_eq!(
        decision.status(),
        RefactorExecuteDecisionStatus::RefusedUnparsableOutput
    );
    assert_eq!(decision.status().reason(), "rewritten-output-did-not-parse");
    assert_eq!(
        decision.status().next_action(),
        "inspect-preview-parse-errors"
    );
    let steps = decision.steps();
    assert_eq!(steps[0].name, "preview-policy");
    assert_eq!(steps[0].status, RefactorExecuteStepStatus::Passed);
    assert_eq!(steps[1].name, "write-output-parse");
    assert_eq!(steps[1].status, RefactorExecuteStepStatus::Failed);
    assert_eq!(steps[2].status, RefactorExecuteStepStatus::Skipped);
    let summary = decision.summary();
    assert_eq!(summary.passed_step_count(), 1);
    assert_eq!(summary.failed_step_count(), 1);
    assert_eq!(summary.skipped_step_count(), 3);
    assert_eq!(summary.scheduled_step_count(), 0);
    assert!(summary.write_parse_refused());
    assert!(!summary.run_pre_verification());
    assert!(!summary.apply_preview());
    assert!(!summary.run_post_verification());
    assert!(!decision.run_pre_verification());
    assert!(!decision.apply_preview());
    assert!(!decision.run_post_verification());
}

#[test]
fn execute_decision_allows_dry_run_preflight_without_post_verification() {
    let decision = build_refactor_execute_preflight_decision(RefactorExecutePreflightInputs::new(
        RefactorExecuteMode::DryRun,
        RefactorExecutePolicyResult::Passed,
        RefactorExecuteOutputParseResult::Unparsable,
    ));

    assert!(!decision.write_parse_refused());
    assert_eq!(
        decision.status(),
        RefactorExecuteDecisionStatus::DryRunReady
    );
    assert_eq!(decision.status().reason(), "all-dry-run-gates-passed");
    assert_eq!(
        decision.status().next_action(),
        "review-preview-or-rerun-with-write"
    );
    let steps = decision.steps();
    assert_eq!(steps[0].status, RefactorExecuteStepStatus::Passed);
    assert_eq!(steps[1].status, RefactorExecuteStepStatus::Passed);
    assert_eq!(steps[2].status, RefactorExecuteStepStatus::Passed);
    assert_eq!(steps[3].status, RefactorExecuteStepStatus::Scheduled);
    assert_eq!(steps[4].status, RefactorExecuteStepStatus::Skipped);
    let summary = decision.summary();
    assert_eq!(summary.passed_step_count(), 3);
    assert_eq!(summary.failed_step_count(), 0);
    assert_eq!(summary.skipped_step_count(), 1);
    assert_eq!(summary.scheduled_step_count(), 1);
    assert!(!summary.write_parse_refused());
    assert!(summary.run_pre_verification());
    assert!(summary.apply_preview());
    assert!(!summary.run_post_verification());
    assert!(decision.run_pre_verification());
    assert!(decision.apply_preview());
    assert!(!decision.run_post_verification());
}

proptest! {
    #[test]
    fn pbt_write_plan_only_allows_changed_parseable_outputs_after_write_request(
        write_requested in any::<bool>(),
        candidates in proptest::collection::vec((any::<bool>(), any::<bool>()), 0..32),
    ) {
        let candidates = candidates
            .into_iter()
            .map(|(changed, output_parse_ok)| RefactorWriteCandidate {
                changed,
                output_parse_ok,
            })
            .collect::<Vec<_>>();
        let plan = build_refactor_write_plan(write_requested, &candidates);
        let parse_error_count = candidates
            .iter()
            .filter(|candidate| !candidate.output_parse_ok)
            .count();

        if !write_requested {
            prop_assert!(!plan.write_allowed());
            prop_assert!(plan.writable_indexes().is_empty());
            prop_assert_eq!(plan.refusal(), None);
        } else if parse_error_count > 0 {
            let expected_refusal = RefactorWriteRefusal::UnparsableOutputs {
                count: parse_error_count,
            };
            prop_assert!(!plan.write_allowed());
            prop_assert!(plan.writable_indexes().is_empty());
            prop_assert_eq!(plan.refusal(), Some(&expected_refusal));
        } else {
            let expected_indexes = candidates
                .iter()
                .enumerate()
                .filter_map(|(index, candidate)| candidate.changed.then_some(index))
                .collect::<Vec<_>>();

            prop_assert!(plan.write_allowed());
            prop_assert_eq!(plan.writable_indexes(), expected_indexes);
            prop_assert_eq!(plan.refusal(), None);
        }
    }

    #[test]
    fn pbt_execute_decision_preserves_gate_order(
        write_requested in any::<bool>(),
        policy_passed in any::<bool>(),
        outputs_parse in any::<bool>(),
        preflight_passed in any::<bool>(),
    ) {
        let inputs = RefactorExecuteGateInputs::new(
            if write_requested {
                RefactorExecuteMode::Write
            } else {
                RefactorExecuteMode::DryRun
            },
            if policy_passed {
                RefactorExecutePolicyResult::Passed
            } else {
                RefactorExecutePolicyResult::Failed
            },
            if outputs_parse {
                RefactorExecuteOutputParseResult::Parseable
            } else {
                RefactorExecuteOutputParseResult::Unparsable
            },
            if preflight_passed {
                RefactorExecutePreVerificationResult::Passed
            } else {
                RefactorExecutePreVerificationResult::Failed
            },
        );
        let decision = build_refactor_execute_decision(inputs);
        let write_parse_refused = policy_passed && write_requested && !outputs_parse;

        prop_assert_eq!(decision.write_parse_refused(), write_parse_refused);
        prop_assert_eq!(
            decision.run_pre_verification(),
            policy_passed && !write_parse_refused,
        );
        prop_assert_eq!(
            decision.apply_preview(),
            policy_passed && !write_parse_refused && preflight_passed,
        );
        prop_assert_eq!(
            decision.run_post_verification(),
            write_requested && policy_passed && !write_parse_refused && preflight_passed,
        );
        let expected_status = if !policy_passed {
            RefactorExecuteDecisionStatus::BlockedByPolicy
        } else if write_parse_refused {
            RefactorExecuteDecisionStatus::RefusedUnparsableOutput
        } else if !preflight_passed {
            RefactorExecuteDecisionStatus::BlockedByPreVerification
        } else if write_requested {
            RefactorExecuteDecisionStatus::ReadyToWrite
        } else {
            RefactorExecuteDecisionStatus::DryRunReady
        };
        prop_assert_eq!(decision.status(), expected_status);
        let steps = decision.steps();
        prop_assert_eq!(steps[0].name, "preview-policy");
        prop_assert_eq!(steps[1].name, "write-output-parse");
        prop_assert_eq!(steps[2].name, "pre-verification");
        prop_assert_eq!(steps[3].name, "apply-preview");
        prop_assert_eq!(steps[4].name, "post-verification");
        prop_assert_eq!(
            steps[0].status,
            if !policy_passed {
                RefactorExecuteStepStatus::Failed
            } else {
                RefactorExecuteStepStatus::Passed
            },
        );
        prop_assert_eq!(
            steps[1].status,
            if !policy_passed {
                RefactorExecuteStepStatus::Skipped
            } else if write_parse_refused {
                RefactorExecuteStepStatus::Failed
            } else {
                RefactorExecuteStepStatus::Passed
            },
        );
        prop_assert_eq!(
            steps[2].status,
            if expected_status == RefactorExecuteDecisionStatus::BlockedByPreVerification {
                RefactorExecuteStepStatus::Failed
            } else if decision.run_pre_verification() {
                RefactorExecuteStepStatus::Passed
            } else {
                RefactorExecuteStepStatus::Skipped
            },
        );
        prop_assert_eq!(
            steps[3].status,
            if decision.apply_preview() {
                RefactorExecuteStepStatus::Scheduled
            } else {
                RefactorExecuteStepStatus::Skipped
            },
        );
        prop_assert_eq!(
            steps[4].status,
            if decision.run_post_verification() {
                RefactorExecuteStepStatus::Scheduled
            } else {
                RefactorExecuteStepStatus::Skipped
            },
        );

        let summary = decision.summary();
        prop_assert_eq!(
            summary.passed_step_count(),
            steps
                .iter()
                .filter(|step| step.status == RefactorExecuteStepStatus::Passed)
                .count(),
        );
        prop_assert_eq!(
            summary.failed_step_count(),
            steps
                .iter()
                .filter(|step| step.status == RefactorExecuteStepStatus::Failed)
                .count(),
        );
        prop_assert_eq!(
            summary.skipped_step_count(),
            steps
                .iter()
                .filter(|step| step.status == RefactorExecuteStepStatus::Skipped)
                .count(),
        );
        prop_assert_eq!(
            summary.scheduled_step_count(),
            steps
                .iter()
                .filter(|step| step.status == RefactorExecuteStepStatus::Scheduled)
                .count(),
        );
        prop_assert_eq!(summary.write_parse_refused(), decision.write_parse_refused());
        prop_assert_eq!(summary.run_pre_verification(), decision.run_pre_verification());
        prop_assert_eq!(summary.apply_preview(), decision.apply_preview());
        prop_assert_eq!(summary.run_post_verification(), decision.run_post_verification());

        prop_assert!(!decision.run_post_verification() || decision.apply_preview());
        prop_assert!(!decision.apply_preview() || decision.run_pre_verification());
        prop_assert!(!decision.run_pre_verification() || !decision.write_parse_refused());
    }
}
