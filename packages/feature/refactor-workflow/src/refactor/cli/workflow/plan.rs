use super::super::super::*;
use super::super::args::*;
use super::super::render::print_refactor_plan;
use super::super::types::plan::{
    RefactorPlan, RefactorPlanPolicyOptions, WorkspaceRefactorPlanDiscovery,
};
use super::shared::derive_refactor_target_kind;
use super::workspace::discover_workspace_refactor_scope;

pub(in crate::presentation::cli) fn refactor_plan(args: RefactorPlanArgs) -> Result<()> {
    emit_refactor_plan(RefactorPlanEmission {
        paths: &args.files,
        dialect: args.dialect,
        symbol: &args.symbol,
        operation: args.operation,
        policy_options: RefactorPlanPolicyOptions {
            fail_on_blocking_gate: args.fail_on_blocking_gate,
            require_definitions: args.require_definitions,
            require_references: args.require_references,
        },
        workspace: None,
        output: args.output,
        failure_label: "refactor plan",
    })
}

pub(in crate::presentation::cli) fn workspace_refactor_plan(
    args: WorkspaceRefactorPlanArgs,
) -> Result<()> {
    let workspace = discover_workspace_refactor_scope(WorkspaceDiscoveryOptions {
        roots: args.roots.clone(),
        include_unknown: args.include_unknown,
        include_hidden: args.include_hidden,
        include_generated: args.include_generated,
        max_depth: args.max_depth,
        exclude: Vec::new(),
    })?;

    emit_refactor_plan(RefactorPlanEmission {
        paths: &workspace.paths,
        dialect: None,
        symbol: &args.symbol,
        operation: args.operation,
        policy_options: RefactorPlanPolicyOptions {
            fail_on_blocking_gate: args.fail_on_blocking_gate,
            require_definitions: args.require_definitions,
            require_references: args.require_references,
        },
        workspace: Some(workspace.workspace),
        output: args.output,
        failure_label: "refactor workspace-plan",
    })
}

struct RefactorPlanEmission<'a> {
    paths: &'a [PathBuf],
    dialect: Option<DialectArg>,
    symbol: &'a SymbolName,
    operation: RefactorOperation,
    policy_options: RefactorPlanPolicyOptions,
    workspace: Option<WorkspaceRefactorPlanDiscovery>,
    output: OutputFormat,
    failure_label: &'static str,
}

fn emit_refactor_plan(request: RefactorPlanEmission<'_>) -> Result<()> {
    let RefactorPlanEmission {
        paths,
        dialect,
        symbol,
        operation,
        policy_options,
        workspace,
        output,
        failure_label,
    } = request;
    let files = impact_report::workflow::collect_impact_reports(paths, dialect, symbol)?;
    let target_kind = derive_refactor_target_kind(&files, symbol.as_str());
    let summary = summarize_impact_reports(&files);
    let operation = ApplicationRefactorOperation::from(operation);
    let decision = build_refactor_plan_decision(RefactorPlanRequest {
        operation,
        symbol: symbol.as_str(),
        files: paths,
        target_kind,
        summary,
        policy: DomainRefactorPlanPolicyOptions::new(
            policy_options.fail_on_blocking_gate,
            policy_options.require_definitions,
            policy_options.require_references,
        )
        .map_err(anyhow::Error::msg)?,
        risks: raw_refactor_risks(&summary),
    });
    let policy = decision.policy;
    let automation = decision.automation;
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");
    let plan = RefactorPlan {
        operation,
        symbol: symbol.as_str().to_owned(),
        target_kind,
        workspace,
        files,
        gates: decision.gates,
        risk_summary: decision.risk_summary,
        steps: decision.steps,
        policy,
        automation,
    };

    print_refactor_plan(&plan, output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "{failure_label} policy failed: {policy_message}"
        )));
    }

    Ok(())
}
