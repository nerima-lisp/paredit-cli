use super::super::super::types::check::RefactorCheckResult;
use super::{RefactorStatusBlockedReason, RefactorStatusNextAction};

pub(in crate::presentation::cli) fn refactor_status_blocked_reasons(
    check: &RefactorCheckResult,
) -> Vec<RefactorStatusBlockedReason> {
    refactor_manifest_blocked_reasons(
        check.manifest_policy_passed,
        check.manifest_outputs_parse,
        check.summary.stale_file_count,
        check.summary.output_hash_mismatch_count,
        check.summary.parse_error_count,
        check.summary.manifest_flag_mismatch_count,
    )
}

pub(in crate::presentation::cli) fn refactor_manifest_blocked_reasons(
    manifest_policy_passed: bool,
    manifest_outputs_parse: bool,
    stale_file_count: usize,
    output_hash_mismatch_count: usize,
    parse_error_count: usize,
    manifest_flag_mismatch_count: usize,
) -> Vec<RefactorStatusBlockedReason> {
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

pub(in crate::presentation::cli) fn refactor_status_next_action(
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
