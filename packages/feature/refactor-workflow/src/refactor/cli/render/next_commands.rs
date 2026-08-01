//! What to run next, for `refactor plan` and `refactor apply`.
//!
//! Both reports already compute everything a suggestion here needs: `plan`
//! carries its own ordered step chain and risk summary, and `apply` carries
//! its own manifest-decision status. Neither function invents a command that
//! is not already implied by data the report holds, in keeping with
//! `paredit_core_cli::report::next_command`'s own rule: a suggestion is only
//! emitted when the report's contents justify it.

use super::super::types::apply::RefactorApplyResult;
use super::super::types::plan::RefactorPlan;
use super::manifest::blocked_reason_text;
use crate::refactor::cli::types::status::{
    RefactorApplyDecisionStatus, RefactorStatusBlockedReason,
};
use paredit_core_cli::report::next_command::{NextCommand, shell_path};

/// Suggestions for `refactor plan`.
///
/// At most two: the step that shows the reason automation cannot proceed
/// (when the plan found a blocking gate) or the step that applies the plan
/// (when it did not), plus whichever step in the plan's own numbered chain
/// comes next after that one. A plan step with no command (a "review this by
/// hand" step) is never suggested, since the point is a command that runs.
#[must_use]
pub fn plan_next_commands(plan: &RefactorPlan) -> Vec<NextCommand> {
    let mut commands: Vec<NextCommand> = Vec::new();
    let mut suggested_order = None;

    if plan.risk_summary.blocking_count > 0 {
        if let Some(step) = plan.steps.iter().find(|step| step.order == 1) {
            if let Some(command) = &step.command {
                commands.push(NextCommand::new(
                    command.clone(),
                    format!(
                        "This plan found {} blocking finding(s); this shows them in context before any edit.",
                        plan.risk_summary.blocking_count
                    ),
                ));
                suggested_order = Some(step.order);
            }
        }
    } else if plan.policy.passed {
        if let Some(step) = plan
            .steps
            .iter()
            .find(|step| step.action.starts_with("apply-"))
        {
            if let Some(command) = &step.command {
                commands.push(NextCommand::new(
                    command.clone(),
                    "No blocking gates were found; this applies the planned refactor.".to_owned(),
                ));
                suggested_order = Some(step.order);
            }
        }
    }

    if plan.steps.len() > 1 {
        let after = suggested_order.unwrap_or(0);
        if let Some(step) = plan
            .steps
            .iter()
            .filter(|step| step.command.is_some() && step.order > after)
            .min_by_key(|step| step.order)
        {
            let command = step.command.clone().expect("filtered on command.is_some");
            if !commands.iter().any(|existing| existing.command == command) {
                commands.push(NextCommand::new(
                    command,
                    format!(
                        "The next step in this plan's own chain (step {}): {}",
                        step.order, step.rationale
                    ),
                ));
            }
        }
    }

    commands
}

/// Suggestions for `refactor apply`.
///
/// One for the manifest's decision status — what re-checking a blocked
/// manifest looks like, or the exact command that would apply a dry run —
/// and, only while the manifest has not yet been fully applied, one more
/// when it carries more than one edit: `refactor step` reviews or applies
/// them individually instead of all at once.
#[must_use]
pub fn apply_next_commands(
    manifest_path: &std::path::Path,
    status: RefactorApplyDecisionStatus,
    blocked_reasons: &[RefactorStatusBlockedReason],
    edit_count: usize,
) -> Vec<NextCommand> {
    let mut commands = Vec::new();
    let manifest_arg = shell_path(&manifest_path.display().to_string());

    match status {
        RefactorApplyDecisionStatus::Blocked => {
            commands.push(NextCommand::new(
                format!("paredit refactor check --manifest {manifest_arg} --output json"),
                format!(
                    "{} blocked reason(s) ({}); this re-checks the manifest and shows why.",
                    blocked_reasons.len(),
                    blocked_reason_text(blocked_reasons)
                ),
            ));
        }
        RefactorApplyDecisionStatus::DryRunReady => {
            commands.push(NextCommand::new(
                format!("paredit refactor apply --manifest {manifest_arg} --write --output json"),
                "This dry run found no blocking issues; this applies it.".to_owned(),
            ));
        }
        RefactorApplyDecisionStatus::Applied => {}
    }

    if edit_count > 1 && !matches!(status, RefactorApplyDecisionStatus::Applied) {
        commands.push(NextCommand::new(
            format!("paredit refactor step --manifest {manifest_arg} --output json"),
            format!(
                "This manifest has {edit_count} edits; refactor step accepts or skips them individually instead of applying all at once."
            ),
        ));
    }

    commands
}

/// Reads [`RefactorApplyResult`]'s own fields into [`apply_next_commands`]'s
/// inputs, so the render call site does not have to know which fields those
/// are.
#[must_use]
pub fn apply_next_commands_for_result(
    result: &RefactorApplyResult,
    status: RefactorApplyDecisionStatus,
    blocked_reasons: &[RefactorStatusBlockedReason],
) -> Vec<NextCommand> {
    apply_next_commands(
        &result.manifest.path,
        status,
        blocked_reasons,
        result.summary.edit_count,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::refactor::usecase::plan::types::RefactorPlanAutomationDecision;
    use crate::refactor::usecase::plan::{
        RefactorOperation, RefactorPlanGate, RefactorPlanPolicy, RefactorPlanRiskSummary,
        RefactorPlanStep, RefactorPlanTargetKind, RefactorRiskLevel,
    };
    use std::path::PathBuf;

    fn base_policy() -> RefactorPlanPolicy {
        RefactorPlanPolicy {
            fail_on_blocking_gate: false,
            require_definitions: None,
            require_references: None,
            blocking_gate_count: 0,
            definition_count: 1,
            reference_count: 1,
            passed: true,
            violations: Vec::new(),
        }
    }

    fn plan_with(
        steps: Vec<RefactorPlanStep>,
        gates: Vec<RefactorPlanGate>,
        passed: bool,
    ) -> RefactorPlan {
        let risk_summary = RefactorPlanRiskSummary::from_gates(&gates);
        let mut policy = base_policy();
        policy.passed = passed;
        RefactorPlan {
            operation: RefactorOperation::Rename,
            symbol: "old-name".to_owned(),
            target_kind: RefactorPlanTargetKind::Callable,
            workspace: None,
            files: Vec::new(),
            gates,
            risk_summary,
            steps,
            policy,
            automation: RefactorPlanAutomationDecision::ready("apply-plan"),
        }
    }

    fn step(order: usize, action: &'static str, command: Option<&str>) -> RefactorPlanStep {
        RefactorPlanStep {
            order,
            action,
            rationale: format!("rationale for {action}"),
            command: command.map(ToOwned::to_owned),
        }
    }

    #[test]
    fn a_blocked_plan_suggests_the_command_showing_the_finding_in_context() {
        let plan = plan_with(
            vec![
                step(
                    1,
                    "run-impact-report",
                    Some("paredit inspect impact --symbol old-name"),
                ),
                step(
                    2,
                    "run-dependency-report",
                    Some("paredit inspect dependencies"),
                ),
            ],
            vec![RefactorPlanGate {
                level: RefactorRiskLevel::Warning,
                code: "ambiguous-definition",
                message: "multiple definitions".to_owned(),
                count: 1,
                blocks_automation: true,
            }],
            false,
        );

        let commands = plan_next_commands(&plan);
        assert_eq!(
            commands[0].command,
            "paredit inspect impact --symbol old-name"
        );
        assert!(commands[0].why.contains("1 blocking"));
    }

    #[test]
    fn a_clean_plan_suggests_the_apply_command() {
        let plan = plan_with(
            vec![
                step(
                    1,
                    "run-impact-report",
                    Some("paredit inspect impact --symbol old-name"),
                ),
                step(
                    3,
                    "apply-rename",
                    Some("paredit refactor rename-function --from old-name --to <new-symbol>"),
                ),
            ],
            Vec::new(),
            true,
        );

        let commands = plan_next_commands(&plan);
        assert_eq!(commands.len(), 1, "{commands:?}");
        assert!(
            commands[0]
                .command
                .starts_with("paredit refactor rename-function")
        );
    }

    #[test]
    fn a_multi_step_plan_suggests_the_next_step_in_its_own_chain_after_the_primary_suggestion() {
        let plan = plan_with(
            vec![
                step(
                    1,
                    "run-impact-report",
                    Some("paredit inspect impact --symbol old-name"),
                ),
                step(
                    2,
                    "run-dependency-report",
                    Some("paredit inspect dependencies"),
                ),
                step(3, "apply-rename", Some("paredit refactor rename-function")),
            ],
            vec![RefactorPlanGate {
                level: RefactorRiskLevel::Warning,
                code: "ambiguous-definition",
                message: "multiple definitions".to_owned(),
                count: 1,
                blocks_automation: true,
            }],
            false,
        );

        let commands = plan_next_commands(&plan);
        assert_eq!(
            commands[0].command,
            "paredit inspect impact --symbol old-name"
        );
        assert_eq!(commands[1].command, "paredit inspect dependencies");
    }

    #[test]
    fn a_single_step_plan_with_no_command_suggests_nothing() {
        let plan = plan_with(
            vec![step(1, "review-rename-scope", None)],
            Vec::new(),
            false,
        );
        assert!(plan_next_commands(&plan).is_empty());
    }

    #[test]
    fn a_blocked_manifest_suggests_recheck() {
        let commands = apply_next_commands(
            &PathBuf::from("manifest.json"),
            RefactorApplyDecisionStatus::Blocked,
            &[RefactorStatusBlockedReason::StaleFiles],
            1,
        );
        assert_eq!(commands.len(), 1);
        assert!(commands[0].command.contains("refactor check"));
        assert!(commands[0].command.contains("manifest.json"));
    }

    #[test]
    fn a_dry_run_ready_manifest_suggests_the_write_rerun() {
        let commands = apply_next_commands(
            &PathBuf::from("manifest.json"),
            RefactorApplyDecisionStatus::DryRunReady,
            &[],
            1,
        );
        assert_eq!(commands.len(), 1);
        assert!(commands[0].command.contains("--write"));
    }

    #[test]
    fn a_multi_edit_dry_run_also_suggests_refactor_step() {
        let commands = apply_next_commands(
            &PathBuf::from("manifest.json"),
            RefactorApplyDecisionStatus::DryRunReady,
            &[],
            3,
        );
        assert_eq!(commands.len(), 2);
        assert!(commands[1].command.contains("refactor step"));
    }

    #[test]
    fn an_applied_single_edit_manifest_suggests_nothing() {
        let commands = apply_next_commands(
            &PathBuf::from("manifest.json"),
            RefactorApplyDecisionStatus::Applied,
            &[],
            1,
        );
        assert!(commands.is_empty());
    }

    #[test]
    fn an_applied_multi_edit_manifest_does_not_suggest_stepping_through_it_again() {
        let commands = apply_next_commands(
            &PathBuf::from("manifest.json"),
            RefactorApplyDecisionStatus::Applied,
            &[],
            3,
        );
        assert!(commands.is_empty());
    }
}
