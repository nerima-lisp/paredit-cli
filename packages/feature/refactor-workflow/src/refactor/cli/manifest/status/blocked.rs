use super::super::super::types::check::RefactorCheckResult;
use super::checks::{
    ManifestOutputs, ManifestPolicy, RefactorManifestChecks, RefactorManifestMismatchCounts,
};
use super::{RefactorStatusBlockedReason, RefactorStatusNextAction};

#[must_use]
pub fn refactor_status_blocked_reasons(
    check: &RefactorCheckResult,
) -> Vec<RefactorStatusBlockedReason> {
    refactor_manifest_blocked_reasons(RefactorManifestChecks {
        policy: ManifestPolicy::from_passed(check.manifest_policy_passed),
        outputs: ManifestOutputs::from_parse(check.manifest_outputs_parse),
        counts: RefactorManifestMismatchCounts {
            stale_files: check.summary.stale_file_count,
            output_hash_mismatches: check.summary.output_hash_mismatch_count,
            parse_errors: check.summary.parse_error_count,
            manifest_flag_mismatches: check.summary.manifest_flag_mismatch_count,
        },
    })
}

#[must_use]
pub fn refactor_manifest_blocked_reasons(
    checks: RefactorManifestChecks,
) -> Vec<RefactorStatusBlockedReason> {
    let manifest_policy_passed = checks.policy.passed();
    let manifest_outputs_parse = checks.outputs.parse();
    let stale_file_count = checks.counts.stale_files;
    let output_hash_mismatch_count = checks.counts.output_hash_mismatches;
    let parse_error_count = checks.counts.parse_errors;
    let manifest_flag_mismatch_count = checks.counts.manifest_flag_mismatches;

    let mut reasons = Vec::new();

    if !manifest_policy_passed {
        reasons.push(RefactorStatusBlockedReason::ManifestPolicyFailed);
    }
    if !manifest_outputs_parse {
        reasons.push(RefactorStatusBlockedReason::ManifestOutputsDoNotParse);
    }
    if stale_file_count > 0 {
        reasons.push(RefactorStatusBlockedReason::StaleFiles);
    }
    if output_hash_mismatch_count > 0 {
        reasons.push(RefactorStatusBlockedReason::OutputHashMismatches);
    }
    if parse_error_count > 0 {
        reasons.push(RefactorStatusBlockedReason::ParseErrors);
    }
    if manifest_flag_mismatch_count > 0 {
        reasons.push(RefactorStatusBlockedReason::ManifestFlagMismatches);
    }

    reasons
}

#[must_use]
pub fn refactor_status_next_action(
    reasons: &[RefactorStatusBlockedReason],
) -> RefactorStatusNextAction {
    if reasons.is_empty() {
        return RefactorStatusNextAction::RunDiffThenApplyWrite;
    }

    if reasons.iter().any(|reason| {
        matches!(
            reason,
            RefactorStatusBlockedReason::ManifestPolicyFailed
                | RefactorStatusBlockedReason::ManifestOutputsDoNotParse
                | RefactorStatusBlockedReason::StaleFiles
                | RefactorStatusBlockedReason::ManifestFlagMismatches
        )
    }) {
        return RefactorStatusNextAction::RegeneratePreview;
    }

    RefactorStatusNextAction::FixManifestOrParser
}
