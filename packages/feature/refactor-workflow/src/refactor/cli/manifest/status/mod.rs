use super::super::types::check::RefactorCheckResult;
use super::super::types::status::{
    RefactorApplyDecision, RefactorApplyDecisionStatus, RefactorApplyNextAction,
    RefactorManifestDecision, RefactorStatusBlockedReason, RefactorStatusKind,
    RefactorStatusNextAction,
};

mod apply;
mod blocked;

pub(in crate::presentation::cli) fn refactor_status_decision(
    check: &RefactorCheckResult,
) -> RefactorManifestDecision {
    refactor_manifest_decision_from_blocked_reasons(refactor_status_blocked_reasons(check))
}

pub(in crate::presentation::cli) fn refactor_manifest_decision(
    manifest_policy_passed: bool,
    manifest_outputs_parse: bool,
    stale_file_count: usize,
    output_hash_mismatch_count: usize,
    parse_error_count: usize,
    manifest_flag_mismatch_count: usize,
) -> RefactorManifestDecision {
    let blocked_reasons = refactor_manifest_blocked_reasons(
        manifest_policy_passed,
        manifest_outputs_parse,
        stale_file_count,
        output_hash_mismatch_count,
        parse_error_count,
        manifest_flag_mismatch_count,
    );
    refactor_manifest_decision_from_blocked_reasons(blocked_reasons)
}

fn refactor_manifest_decision_from_blocked_reasons(
    blocked_reasons: Vec<RefactorStatusBlockedReason>,
) -> RefactorManifestDecision {
    let status = if blocked_reasons.is_empty() {
        RefactorStatusKind::Ready
    } else {
        RefactorStatusKind::Blocked
    };
    let next_action = refactor_status_next_action(&blocked_reasons);

    RefactorManifestDecision {
        status,
        next_action,
        blocked_reasons,
    }
}

pub(in crate::presentation::cli) fn refactor_apply_decision(
    manifest_policy_passed: bool,
    manifest_outputs_parse: bool,
    stale_file_count: usize,
    output_hash_mismatch_count: usize,
    parse_error_count: usize,
    manifest_flag_mismatch_count: usize,
    applied: bool,
) -> RefactorApplyDecision {
    let decision = refactor_manifest_decision(
        manifest_policy_passed,
        manifest_outputs_parse,
        stale_file_count,
        output_hash_mismatch_count,
        parse_error_count,
        manifest_flag_mismatch_count,
    );

    let (status, next_action) = refactor_apply_status_and_action(&decision, applied);

    RefactorApplyDecision {
        status,
        next_action,
        blocked_reasons: decision.blocked_reasons,
    }
}

pub(in crate::presentation::cli) use apply::refactor_apply_status_and_action;
pub(in crate::presentation::cli) use blocked::{
    refactor_manifest_blocked_reasons, refactor_status_blocked_reasons, refactor_status_next_action,
};

#[cfg(test)]
mod tests;
