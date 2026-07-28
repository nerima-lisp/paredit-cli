use super::{
    RefactorApplyDecisionStatus, RefactorApplyNextAction, RefactorManifestDecision,
    RefactorStatusNextAction,
};

#[must_use]
pub fn refactor_apply_status_and_action(
    decision: &RefactorManifestDecision,
    applied: bool,
) -> (RefactorApplyDecisionStatus, RefactorApplyNextAction) {
    if applied {
        return (
            RefactorApplyDecisionStatus::Applied,
            RefactorApplyNextAction::RunVerificationOrReviewDiff,
        );
    }

    if decision.blocked_reasons.is_empty() {
        return (
            RefactorApplyDecisionStatus::DryRunReady,
            RefactorApplyNextAction::RerunWithWrite,
        );
    }

    (
        RefactorApplyDecisionStatus::Blocked,
        refactor_apply_next_action_from_status(decision.next_action),
    )
}

const fn refactor_apply_next_action_from_status(
    next_action: RefactorStatusNextAction,
) -> RefactorApplyNextAction {
    match next_action {
        RefactorStatusNextAction::RunDiffThenApplyWrite => RefactorApplyNextAction::RerunWithWrite,
        RefactorStatusNextAction::RegeneratePreview => RefactorApplyNextAction::RegeneratePreview,
        RefactorStatusNextAction::FixManifestOrParser => {
            RefactorApplyNextAction::FixManifestOrParser
        }
    }
}
