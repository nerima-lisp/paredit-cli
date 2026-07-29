use paredit_core_cli::CliResult;

use crate::presentation::cli::dependency_report::{
    args::DependencyReportArgs,
    render::{dependency_drawing, print_dependency_report},
    types::DependencyReportFile,
};
use crate::presentation::cli::read_input_dialect_and_tree;
use paredit_core_cli::report::graph::print_graph;
use paredit_feature_package::dependency_report::usecase::build_dependency_report;
use paredit_feature_remove_unused::definition_report::usecase::collect_definition_forms;

pub fn dependency_report(args: DependencyReportArgs) -> CliResult<()> {
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
