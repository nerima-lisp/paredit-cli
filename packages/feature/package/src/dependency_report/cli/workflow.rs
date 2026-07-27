use anyhow::Result;

use crate::application::usecase::definition_report::collect_definition_forms;
use crate::application::usecase::dependency_report::build_dependency_report;
use crate::presentation::cli::dependency_report::{
    args::DependencyReportArgs, render::print_dependency_report, types::DependencyReportFile,
};
use crate::presentation::cli::shared::read_input_dialect_and_tree;

pub(in crate::presentation::cli) fn dependency_report(args: DependencyReportArgs) -> Result<()> {
    let mut reports = Vec::with_capacity(args.files.len());

    for file in &args.files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (package, _) = collect_definition_forms(&tree, dialect)?;
        let dependency_report = build_dependency_report(&tree, dialect)?;

        reports.push(DependencyReportFile {
            path: file.clone(),
            dialect,
            package,
            dependencies: dependency_report.dependencies,
        });
    }

    print_dependency_report(&reports, args.output)
}
