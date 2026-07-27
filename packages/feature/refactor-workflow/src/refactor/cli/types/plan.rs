use crate::refactor::usecase::plan::{
    RefactorOperation as ApplicationRefactorOperation, RefactorPlanGate, RefactorPlanPolicy,
    RefactorPlanStep,
};
use crate::refactor::usecase::plan::{
    RefactorPlanAutomationDecision, RefactorPlanRiskSummary, RefactorPlanTargetKind,
};
use paredit_feature_project_analysis::impact_report::usecase::ImpactReportFile;
use std::path::PathBuf;

#[derive(Debug)]
pub struct RefactorPlan {
    pub operation: ApplicationRefactorOperation,
    pub symbol: String,
    pub target_kind: RefactorPlanTargetKind,
    pub workspace: Option<WorkspaceRefactorPlanDiscovery>,
    pub files: Vec<ImpactReportFile>,
    pub gates: Vec<RefactorPlanGate>,
    pub risk_summary: RefactorPlanRiskSummary,
    pub steps: Vec<RefactorPlanStep>,
    pub policy: RefactorPlanPolicy,
    pub automation: RefactorPlanAutomationDecision,
}

#[derive(Debug)]
pub struct WorkspaceRefactorPlanDiscovery {
    pub roots: Vec<PathBuf>,
    pub discovered_file_count: usize,
    pub skipped_unknown_count: usize,
    pub skipped_hidden_count: usize,
    pub skipped_generated_count: usize,
    pub skipped_symlink_count: usize,
}

#[derive(Debug)]
pub struct RefactorPlanPolicyOptions {
    pub fail_on_blocking_gate: bool,
    pub require_definitions: Option<usize>,
    pub require_references: Option<usize>,
}
