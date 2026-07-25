use std::path::PathBuf;

use crate::domain::dialect::Dialect;
pub use crate::domain::workspace_report::{
    WorkspaceFileMetrics, WorkspaceFileStatus, WorkspaceReportSummary,
};

#[derive(Debug)]
pub struct WorkspaceReportRequest {
    pub roots: Vec<PathBuf>,
    pub include_unknown: bool,
    pub include_hidden: bool,
    pub include_generated: bool,
    pub max_depth: Option<usize>,
}

#[derive(Debug)]
pub struct WorkspaceInventory {
    pub files: Vec<PathBuf>,
    pub skipped_unknown_count: usize,
    pub skipped_hidden_count: usize,
    pub skipped_generated_count: usize,
    pub skipped_symlink_count: usize,
}

#[derive(Debug)]
pub struct LoadedWorkspaceFile {
    pub dialect: Dialect,
    pub bytes: Result<Vec<u8>, String>,
}

pub trait WorkspaceReportSourcePort {
    fn discover(&mut self, request: &WorkspaceReportRequest) -> anyhow::Result<WorkspaceInventory>;

    fn load(&self, path: &std::path::Path) -> LoadedWorkspaceFile;
}

#[derive(Debug)]
pub struct WorkspaceFileReport {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub status: WorkspaceFileStatus,
    pub byte_count: usize,
    pub top_level_form_count: usize,
    pub atom_count: usize,
    pub definition_count: usize,
    pub call_count: usize,
    pub max_complexity_score: usize,
    pub package: Option<String>,
}

#[derive(Debug)]
pub struct WorkspaceReportPlan {
    pub roots: Vec<PathBuf>,
    pub reports: Vec<WorkspaceFileReport>,
    pub summary: WorkspaceReportSummary,
    pub skipped_unknown_count: usize,
    pub skipped_hidden_count: usize,
    pub skipped_generated_count: usize,
    pub skipped_symlink_count: usize,
}
