pub mod apply;
pub mod check;
pub mod diff;
pub mod status;
pub mod undo;

use super::super::types::status::{
    RefactorManifestDecisionStep, RefactorManifestDecisionSummary, RefactorStatusBlockedReason,
};

#[must_use]
pub fn blocked_reason_text(blocked_reasons: &[RefactorStatusBlockedReason]) -> String {
    blocked_reason_labels(blocked_reasons).join(",")
}

#[must_use]
pub fn blocked_reason_labels(blocked_reasons: &[RefactorStatusBlockedReason]) -> Vec<&'static str> {
    blocked_reasons
        .iter()
        .map(|reason| reason.label())
        .collect()
}

pub fn decision_steps_json(
    steps: impl IntoIterator<Item = RefactorManifestDecisionStep>,
) -> Vec<serde_json::Value> {
    steps
        .into_iter()
        .map(|step| {
            serde_json::json!({
                "name": step.name,
                "status": step.status.label(),
            })
        })
        .collect()
}

#[must_use]
pub fn decision_summary_json(summary: RefactorManifestDecisionSummary) -> serde_json::Value {
    serde_json::json!({
        "passed_step_count": summary.passed_step_count,
        "failed_step_count": summary.failed_step_count,
        "skipped_step_count": summary.skipped_step_count,
        "scheduled_step_count": summary.scheduled_step_count,
        "blocked_reason_count": summary.blocked_reason_count,
    })
}

pub fn print_decision_summary(summary: RefactorManifestDecisionSummary) {
    println!("passed_step_count\t{}", summary.passed_step_count);
    println!("failed_step_count\t{}", summary.failed_step_count);
    println!("skipped_step_count\t{}", summary.skipped_step_count);
    println!("scheduled_step_count\t{}", summary.scheduled_step_count);
    println!("blocked_reason_count\t{}", summary.blocked_reason_count);
}
