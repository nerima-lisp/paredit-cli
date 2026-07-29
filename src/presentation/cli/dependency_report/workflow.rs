use anyhow::Result;

use crate::application::usecase::definition_report::collect_definition_forms;
use crate::application::usecase::dependency_report::build_dependency_report;
use crate::presentation::cli::dependency_report::{
    args::DependencyReportArgs,
    render::{dependency_drawing, print_dependency_report},
    types::DependencyReportFile,
};
use crate::presentation::cli::read_input_dialect_and_tree;
use paredit_core_cli::report::graph::print_graph;

pub fn dependency_report(args: DependencyReportArgs) -> Result<()> {
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

    match args.graph {
        Some(format) => {
            print_graph(&dependency_drawing(&reports), format);
            Ok(())
        }
        None => print_dependency_report(&reports, args.output),
    }
}
