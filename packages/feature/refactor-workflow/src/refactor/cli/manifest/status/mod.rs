use super::super::types::check::RefactorCheckResult;
use super::super::types::status::{
    RefactorApplyDecision, RefactorApplyDecisionStatus, RefactorApplyNextAction,
    RefactorManifestDecision, RefactorStatusBlockedReason, RefactorStatusKind,
    RefactorStatusNextAction,
};

pub mod apply;
pub mod blocked;
pub mod checks;

#[must_use]
pub fn refactor_status_decision(check: &RefactorCheckResult) -> RefactorManifestDecision {
    refactor_manifest_decision_from_blocked_reasons(refactor_status_blocked_reasons(check))
}

#[must_use]
pub fn refactor_manifest_decision(checks: RefactorManifestChecks) -> RefactorManifestDecision {
    let blocked_reasons = refactor_manifest_blocked_reasons(checks);
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

#[must_use]
pub fn refactor_apply_decision(
    checks: RefactorManifestChecks,
    outcome: RefactorApplyOutcome,
) -> RefactorApplyDecision {
    let decision = refactor_manifest_decision(checks);

    let (status, next_action) = refactor_apply_status_and_action(&decision, outcome.applied());

    RefactorApplyDecision {
        status,
        next_action,
        blocked_reasons: decision.blocked_reasons,
    }
}

pub use apply::refactor_apply_status_and_action;
pub use blocked::{
    refactor_manifest_blocked_reasons, refactor_status_blocked_reasons, refactor_status_next_action,
};
pub use checks::{
    ManifestOutputs, ManifestPolicy, RefactorApplyOutcome, RefactorManifestChecks,
    RefactorManifestMismatchCounts,
};

#[cfg(test)]
pub mod tests;
