use std::path::PathBuf;

use proptest::prelude::*;

use super::*;
use crate::presentation::cli::refactor::types::check::RefactorCheckSummary;
use crate::presentation::cli::refactor::types::manifest::RefactorApplyManifestHeader;
use crate::presentation::cli::refactor::types::root::RefactorRootReport;
use crate::presentation::cli::refactor::types::status::RefactorManifestDecisionStepStatus;

fn count_step_status(
    steps: &[crate::presentation::cli::refactor::types::status::RefactorManifestDecisionStep],
    status: RefactorManifestDecisionStepStatus,
) -> usize {
    steps.iter().filter(|step| step.status == status).count()
}

fn check_result(
    manifest_policy_passed: bool,
    manifest_outputs_parse: bool,
    stale_file_count: usize,
    output_hash_mismatch_count: usize,
    parse_error_count: usize,
    manifest_flag_mismatch_count: usize,
) -> RefactorCheckResult {
    RefactorCheckResult {
        manifest: RefactorApplyManifestHeader {
            path: PathBuf::from("refactor preview.json"),
            hash: "manifest-hash".to_string(),
            mode: "rename-symbol".to_string(),
            from: "old-name".to_string(),
            to: "new-name".to_string(),
        },
        root: RefactorRootReport {
            enforced: false,
            path: None,
        },
        manifest_policy_passed,
        manifest_outputs_parse,
        files: Vec::new(),
        summary: RefactorCheckSummary {
            file_count: 0,
            changed_file_count: 0,
            changed_files: Vec::new(),
            edit_count: 0,
            stale_file_count,
            output_hash_mismatch_count,
            parse_error_count,
            manifest_flag_mismatch_count,
            can_apply: manifest_policy_passed
                && manifest_outputs_parse
                && stale_file_count == 0
                && output_hash_mismatch_count == 0
                && parse_error_count == 0
                && manifest_flag_mismatch_count == 0,
        },
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    #[test]
    fn pbt_blocked_reasons_are_exactly_the_failed_check_inputs(
        manifest_policy_passed in any::<bool>(),
        manifest_outputs_parse in any::<bool>(),
        stale_file_count in 0usize..4,
        output_hash_mismatch_count in 0usize..4,
        parse_error_count in 0usize..4,
        manifest_flag_mismatch_count in 0usize..4,
    ) {
        let check = check_result(
            manifest_policy_passed,
            manifest_outputs_parse,
            stale_file_count,
            output_hash_mismatch_count,
            parse_error_count,
            manifest_flag_mismatch_count,
        );
        let reasons = refactor_status_blocked_reasons(&check);

        prop_assert_eq!(
            reasons.contains(&RefactorStatusBlockedReason::ManifestPolicyFailed),
            !manifest_policy_passed
        );
        prop_assert_eq!(
            reasons.contains(&RefactorStatusBlockedReason::ManifestOutputsDoNotParse),
            !manifest_outputs_parse
        );
        prop_assert_eq!(
            reasons.contains(&RefactorStatusBlockedReason::StaleFiles),
            stale_file_count > 0
        );
        prop_assert_eq!(
            reasons.contains(&RefactorStatusBlockedReason::OutputHashMismatches),
            output_hash_mismatch_count > 0
        );
        prop_assert_eq!(
            reasons.contains(&RefactorStatusBlockedReason::ParseErrors),
            parse_error_count > 0
        );
        prop_assert_eq!(
            reasons.contains(&RefactorStatusBlockedReason::ManifestFlagMismatches),
            manifest_flag_mismatch_count > 0
        );
        prop_assert_eq!(reasons.is_empty(), check.summary.can_apply);
    }

    #[test]
    fn pbt_next_action_priority_is_stable(
        manifest_policy_failed in any::<bool>(),
        manifest_outputs_do_not_parse in any::<bool>(),
        stale_files in any::<bool>(),
        output_hash_mismatches in any::<bool>(),
        parse_errors in any::<bool>(),
        manifest_flag_mismatches in any::<bool>(),
    ) {
        let mut reasons = Vec::new();
        if manifest_policy_failed {
            reasons.push(RefactorStatusBlockedReason::ManifestPolicyFailed);
        }
        if manifest_outputs_do_not_parse {
            reasons.push(RefactorStatusBlockedReason::ManifestOutputsDoNotParse);
        }
        if stale_files {
            reasons.push(RefactorStatusBlockedReason::StaleFiles);
        }
        if output_hash_mismatches {
            reasons.push(RefactorStatusBlockedReason::OutputHashMismatches);
        }
        if parse_errors {
            reasons.push(RefactorStatusBlockedReason::ParseErrors);
        }
        if manifest_flag_mismatches {
            reasons.push(RefactorStatusBlockedReason::ManifestFlagMismatches);
        }

        let expected = if reasons.is_empty() {
            RefactorStatusNextAction::RunDiffThenApplyWrite
        } else if manifest_policy_failed
            || manifest_outputs_do_not_parse
            || stale_files
            || manifest_flag_mismatches
        {
            RefactorStatusNextAction::RegeneratePreview
        } else {
            RefactorStatusNextAction::FixManifestOrParser
        };

        prop_assert_eq!(refactor_status_next_action(&reasons), expected);
    }

    #[test]
    fn pbt_manifest_decision_matches_blocked_reasons_and_next_action(
        manifest_policy_passed in any::<bool>(),
        manifest_outputs_parse in any::<bool>(),
        stale_file_count in 0usize..4,
        output_hash_mismatch_count in 0usize..4,
        parse_error_count in 0usize..4,
        manifest_flag_mismatch_count in 0usize..4,
    ) {
        let decision = refactor_manifest_decision(
            manifest_policy_passed,
            manifest_outputs_parse,
            stale_file_count,
            output_hash_mismatch_count,
            parse_error_count,
            manifest_flag_mismatch_count,
        );

        let expected_status = if decision.blocked_reasons.is_empty() {
            RefactorStatusKind::Ready
        } else {
            RefactorStatusKind::Blocked
        };

        prop_assert_eq!(decision.status, expected_status);
        prop_assert_eq!(
            decision.next_action,
            refactor_status_next_action(&decision.blocked_reasons)
        );

        let steps = decision.steps();
        prop_assert_eq!(steps[0].name, "manifest-policy");
        prop_assert_eq!(
            steps[0].status,
            if manifest_policy_passed {
                RefactorManifestDecisionStepStatus::Passed
            } else {
                RefactorManifestDecisionStepStatus::Failed
            }
        );
        prop_assert_eq!(steps[1].name, "manifest-outputs-parse");
        prop_assert_eq!(
            steps[1].status,
            if manifest_outputs_parse {
                RefactorManifestDecisionStepStatus::Passed
            } else {
                RefactorManifestDecisionStepStatus::Failed
            }
        );
        prop_assert_eq!(steps[2].name, "file-freshness");
        prop_assert_eq!(
            steps[2].status,
            if stale_file_count == 0 {
                RefactorManifestDecisionStepStatus::Passed
            } else {
                RefactorManifestDecisionStepStatus::Failed
            }
        );
        prop_assert_eq!(steps[3].name, "output-validation");
        prop_assert_eq!(
            steps[3].status,
            if output_hash_mismatch_count == 0
                && parse_error_count == 0
                && manifest_flag_mismatch_count == 0
            {
                RefactorManifestDecisionStepStatus::Passed
            } else {
                RefactorManifestDecisionStepStatus::Failed
            }
        );
        prop_assert_eq!(steps[4].name, "apply-write");
        prop_assert_eq!(
            steps[4].status,
            if decision.blocked_reasons.is_empty() {
                RefactorManifestDecisionStepStatus::Scheduled
            } else {
                RefactorManifestDecisionStepStatus::Skipped
            }
        );

        let summary = decision.summary();
        prop_assert_eq!(
            summary.passed_step_count,
            count_step_status(&steps, RefactorManifestDecisionStepStatus::Passed)
        );
        prop_assert_eq!(
            summary.failed_step_count,
            count_step_status(&steps, RefactorManifestDecisionStepStatus::Failed)
        );
        prop_assert_eq!(
            summary.skipped_step_count,
            count_step_status(&steps, RefactorManifestDecisionStepStatus::Skipped)
        );
        prop_assert_eq!(
            summary.scheduled_step_count,
            count_step_status(&steps, RefactorManifestDecisionStepStatus::Scheduled)
        );
        prop_assert_eq!(
            summary.blocked_reason_count,
            decision.blocked_reasons.len()
        );
    }

    #[test]
    fn pbt_apply_decision_keeps_operation_aware_status_contract(
        manifest_policy_passed in any::<bool>(),
        manifest_outputs_parse in any::<bool>(),
        stale_file_count in 0usize..4,
        output_hash_mismatch_count in 0usize..4,
        parse_error_count in 0usize..4,
        manifest_flag_mismatch_count in 0usize..4,
        applied in any::<bool>(),
    ) {
        let manifest_decision = refactor_manifest_decision(
            manifest_policy_passed,
            manifest_outputs_parse,
            stale_file_count,
            output_hash_mismatch_count,
            parse_error_count,
            manifest_flag_mismatch_count,
        );
        let apply_decision = refactor_apply_decision(
            manifest_policy_passed,
            manifest_outputs_parse,
            stale_file_count,
            output_hash_mismatch_count,
            parse_error_count,
            manifest_flag_mismatch_count,
            applied,
        );

        let expected_status = if applied {
            RefactorApplyDecisionStatus::Applied
        } else if manifest_decision.blocked_reasons.is_empty() {
            RefactorApplyDecisionStatus::DryRunReady
        } else {
            RefactorApplyDecisionStatus::Blocked
        };
        let expected_next_action = if applied {
            RefactorApplyNextAction::RunVerificationOrReviewDiff
        } else if manifest_decision.blocked_reasons.is_empty() {
            RefactorApplyNextAction::RerunWithWrite
        } else if manifest_decision.next_action == RefactorStatusNextAction::RegeneratePreview {
            RefactorApplyNextAction::RegeneratePreview
        } else {
            RefactorApplyNextAction::FixManifestOrParser
        };

        prop_assert_eq!(apply_decision.status, expected_status);
        prop_assert_eq!(apply_decision.next_action, expected_next_action);
        prop_assert_eq!(
            apply_decision.steps()[4].status,
            if applied {
                RefactorManifestDecisionStepStatus::Passed
            } else if manifest_decision.blocked_reasons.is_empty() {
                RefactorManifestDecisionStepStatus::Scheduled
            } else {
                RefactorManifestDecisionStepStatus::Skipped
            }
        );
        prop_assert_eq!(
            &apply_decision.blocked_reasons,
            &manifest_decision.blocked_reasons
        );

        let steps = apply_decision.steps();
        let summary = apply_decision.summary();
        prop_assert_eq!(
            summary.passed_step_count,
            count_step_status(&steps, RefactorManifestDecisionStepStatus::Passed)
        );
        prop_assert_eq!(
            summary.failed_step_count,
            count_step_status(&steps, RefactorManifestDecisionStepStatus::Failed)
        );
        prop_assert_eq!(
            summary.skipped_step_count,
            count_step_status(&steps, RefactorManifestDecisionStepStatus::Skipped)
        );
        prop_assert_eq!(
            summary.scheduled_step_count,
            count_step_status(&steps, RefactorManifestDecisionStepStatus::Scheduled)
        );
        prop_assert_eq!(
            summary.blocked_reason_count,
            apply_decision.blocked_reasons.len()
        );
    }
}
