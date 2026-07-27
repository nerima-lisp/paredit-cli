use std::path::Path;

use anyhow::Result;

use crate::application::usecase::workspace_report::types::{
    LoadedWorkspaceFile, WorkspaceInventory, WorkspaceReportRequest, WorkspaceReportSourcePort,
};
use crate::application::usecase::workspace_report::workflow::build_workspace_report;
use crate::domain::dialect::Dialect;
use crate::infrastructure::workspace::{
    WorkspaceDiscovery, WorkspaceDiscoveryOptions, discover_workspace_files,
};

use super::args::WorkspaceReportArgs;
use super::render::print_workspace_report;

pub(in crate::presentation::cli) fn workspace_report(args: WorkspaceReportArgs) -> Result<()> {
    let output = args.output;
    let request = WorkspaceReportRequest {
        roots: args.roots,
        include_unknown: args.include_unknown,
        include_hidden: args.include_hidden,
        include_generated: args.include_generated,
        max_depth: args.max_depth,
    };
    let mut source = CliWorkspaceReportSource::default();
    let plan = build_workspace_report(&mut source, request)?;
    print_workspace_report(&plan, output)
}

#[derive(Default)]
struct CliWorkspaceReportSource {
    discovery: Option<WorkspaceDiscovery>,
}

impl WorkspaceReportSourcePort for CliWorkspaceReportSource {
    fn discover(&mut self, request: &WorkspaceReportRequest) -> Result<WorkspaceInventory> {
        let discovery = discover_workspace_files(&WorkspaceDiscoveryOptions {
            roots: request.roots.clone(),
            include_unknown: request.include_unknown,
            include_hidden: request.include_hidden,
            include_generated: request.include_generated,
            max_depth: request.max_depth,
            exclude: Vec::new(),
        })?;
        let inventory = WorkspaceInventory {
            files: discovery.files().to_vec(),
            skipped_unknown_count: discovery.skipped_unknown_count(),
            skipped_hidden_count: discovery.skipped_hidden_count(),
            skipped_generated_count: discovery.skipped_generated_count(),
            skipped_symlink_count: discovery.skipped_symlink_count(),
        };
        self.discovery = Some(discovery);
        Ok(inventory)
    }

    fn load(&self, path: &Path) -> LoadedWorkspaceFile {
        let dialect = Dialect::detect(Some(path), None);
        let bytes = self
            .discovery
            .as_ref()
            .expect("workspace discovery must precede file loading")
            .read_file(path)
            .map_err(|error| error.to_string());
        LoadedWorkspaceFile { dialect, bytes }
    }
}
