use super::super::super::*;
use crate::application::refactor::plan::{
    RefactorPlanAutomationDecision, RefactorPlanRiskSummary, RefactorPlanTargetKind,
};

#[derive(Debug)]
pub(in crate::presentation::cli) struct RefactorPlan {
    pub(in crate::presentation::cli) operation: ApplicationRefactorOperation,
    pub(in crate::presentation::cli) symbol: String,
    pub(in crate::presentation::cli) target_kind: RefactorPlanTargetKind,
    pub(in crate::presentation::cli) workspace: Option<WorkspaceRefactorPlanDiscovery>,
    pub(in crate::presentation::cli) files: Vec<ImpactReportFile>,
    pub(in crate::presentation::cli) gates: Vec<RefactorPlanGate>,
    pub(in crate::presentation::cli) risk_summary: RefactorPlanRiskSummary,
    pub(in crate::presentation::cli) steps: Vec<RefactorPlanStep>,
    pub(in crate::presentation::cli) policy: RefactorPlanPolicy,
    pub(in crate::presentation::cli) automation: RefactorPlanAutomationDecision,
}

#[derive(Debug)]
pub(in crate::presentation::cli) struct WorkspaceRefactorPlanDiscovery {
    pub(in crate::presentation::cli) roots: Vec<PathBuf>,
    pub(in crate::presentation::cli) discovered_file_count: usize,
    pub(in crate::presentation::cli) skipped_unknown_count: usize,
    pub(in crate::presentation::cli) skipped_hidden_count: usize,
    pub(in crate::presentation::cli) skipped_generated_count: usize,
    pub(in crate::presentation::cli) skipped_symlink_count: usize,
}

#[derive(Debug)]
pub(in crate::presentation::cli) struct RefactorPlanPolicyOptions {
    pub(in crate::presentation::cli) fail_on_blocking_gate: bool,
    pub(in crate::presentation::cli) require_definitions: Option<usize>,
    pub(in crate::presentation::cli) require_references: Option<usize>,
}
