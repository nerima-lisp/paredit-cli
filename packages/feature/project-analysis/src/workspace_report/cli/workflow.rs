use std::path::Path;

use anyhow::Result;

use crate::workspace_report::usecase::types::{
    LoadedWorkspaceFile, WorkspaceInventory, WorkspaceReportRequest, WorkspaceReportSourcePort,
};
use crate::workspace_report::usecase::workflow::build_workspace_report;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_workspace::workspace::{
    WorkspaceDiscovery, WorkspaceDiscoveryOptions, discover_workspace_files,
    discover_workspace_files_from_list,
};

use super::args::WorkspaceReportArgs;
use super::render::print_workspace_report;

pub fn workspace_report(args: WorkspaceReportArgs) -> Result<()> {
    let output = args.output;
    // Resolution runs here, not in the use case: `--since` shells out to git and
    // `--paths-from -` reads stdin, and a use case that did either would stop
    // being testable without a filesystem and a repository.
    let resolved = args.input.resolve(&args.roots)?;
    let request = WorkspaceReportRequest {
        roots: args.roots,
        include_unknown: args.input.include_unknown,
        include_hidden: args.input.include_hidden,
        include_generated: args.input.include_generated,
        max_depth: args.input.max_depth,
    };
    let mut source = CliWorkspaceReportSource {
        options: resolved.options,
        from_list: resolved.from_list,
        discovery: None,
    };
    let plan = build_workspace_report(&mut source, request)?;
    print_workspace_report(&plan, output)
}

struct CliWorkspaceReportSource {
    options: WorkspaceDiscoveryOptions,
    from_list: bool,
    discovery: Option<WorkspaceDiscovery>,
}

impl WorkspaceReportSourcePort for CliWorkspaceReportSource {
    fn discover(&mut self, _request: &WorkspaceReportRequest) -> Result<WorkspaceInventory> {
        let discovery = if self.from_list {
            discover_workspace_files_from_list(&self.options)?
        } else {
            discover_workspace_files(&self.options)?
        };
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
