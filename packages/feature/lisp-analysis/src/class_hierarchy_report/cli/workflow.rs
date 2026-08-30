use paredit_core_cli::{CliResult, CommandResult};

use paredit_core_cli::shared::{
    analyze_files, expand_input_files, note_partial_file_failures, total_file_failure,
};

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

    let analysis = analyze_files(&files, args.dialect, |file, dialect, tree, _| {
        CliResult::Ok(build_class_hierarchy_report(file, dialect, tree))
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

    let policy = evaluate_fail_on_shadowed_slot_policy(args.fail_on_shadowed_slot, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    match args.graph {
        Some(format) => print_graph(&class_hierarchy_drawing(&reports), format),
        None => print_shadowed_slot_report(&reports, &policy, args.output, args.verbosity)?,
    }

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect class-hierarchy policy failed: {message}"
        )));
    }

    Ok(())
}
