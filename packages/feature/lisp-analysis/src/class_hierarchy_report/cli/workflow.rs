use paredit_core_cli::CommandResult;

use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::class_hierarchy_report::cli::args::ClassHierarchyReportArgs;
use crate::class_hierarchy_report::cli::render::{
    class_hierarchy_drawing, print_shadowed_slot_report,
};
use crate::class_hierarchy_report::usecase::{
    build_class_hierarchy_report, evaluate_fail_on_shadowed_slot_policy,
};
use paredit_core_cli::report::graph::print_graph;

pub fn class_hierarchy_report(args: ClassHierarchyReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_class_hierarchy_report(file, dialect, &tree));
    }

    let policy = evaluate_fail_on_shadowed_slot_policy(args.fail_on_shadowed_slot, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    match args.graph {
        Some(format) => print_graph(&class_hierarchy_drawing(&reports), format),
        None => print_shadowed_slot_report(&reports, &policy, args.output)?,
    }

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect class-hierarchy policy failed: {message}"
        )));
    }

    Ok(())
}
