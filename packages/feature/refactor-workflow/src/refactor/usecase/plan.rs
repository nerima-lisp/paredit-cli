pub mod steps;
#[cfg(test)]
pub mod tests;
pub mod types;

pub use paredit_core_edit::refactor_plan::{
    RawRefactorRisk, RefactorOperation, RefactorPlanGate, RefactorPlanOverallRisk,
    RefactorPlanPolicy, RefactorPlanPolicyOptions, RefactorPlanRiskSummary, RefactorPlanSummary,
    RefactorPlanTargetKind, RefactorRiskLevel, RefactorVerificationCheck,
    RefactorVerificationRequest, VerificationPhase, evaluate_refactor_plan_policy,
    refactor_plan_gates, refactor_verification_checks,
};
pub use steps::refactor_plan_steps;
pub use types::{
    RefactorPlanAutomationDecision, RefactorPlanAutomationStatus, RefactorPlanAutomationStep,
    RefactorPlanAutomationStepStatus, RefactorPlanDecision, RefactorPlanRequest, RefactorPlanStep,
};

#[must_use]
pub fn build_refactor_plan_decision(request: RefactorPlanRequest<'_>) -> RefactorPlanDecision {
    let gates = refactor_plan_gates(
        request.operation,
        request.target_kind,
        &request.summary,
        request.risks,
    );
    let risk_summary = RefactorPlanRiskSummary::from_gates(&gates);
    let steps = refactor_plan_steps(
        request.operation,
        request.symbol,
        request.files,
        request.target_kind,
        &gates,
    );
    let policy = evaluate_refactor_plan_policy(request.policy, &request.summary, &gates);
    let automation = refactor_plan_automation_decision(&policy, &steps);

    RefactorPlanDecision {
        gates,
        risk_summary,
        steps,
        policy,
        automation,
    }
}

#[must_use]
pub fn refactor_plan_automation_decision(
    policy: &RefactorPlanPolicy,
    steps: &[RefactorPlanStep],
) -> RefactorPlanAutomationDecision {
    let apply_action = steps
        .iter()
        .find(|step| step.order == 3)
        .map(|step| step.action)
        .unwrap_or("review-plan");

    if !policy.passed {
        return RefactorPlanAutomationDecision::policy_failed(
            policy
                .violations
                .first()
                .cloned()
                .unwrap_or_else(|| "refactor plan policy failed".to_owned()),
            policy.blocking_gate_count,
        );
    }

    if apply_action.starts_with("review-") {
        return RefactorPlanAutomationDecision::manual_review(
            format!("{apply_action} requires manual review before automated edits"),
            apply_action,
            policy.blocking_gate_count,
        );
    }

    if policy.blocking_gate_count > 0 {
        return RefactorPlanAutomationDecision::manual_review(
            format!(
                "{} blocking gate(s) require manual review before automated edits",
                policy.blocking_gate_count
            ),
            apply_action,
            policy.blocking_gate_count,
        );
    }

    RefactorPlanAutomationDecision::ready(apply_action)
}
