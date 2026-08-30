use paredit_core_cli::CommandResult;

use crate::class_cycle_report::cli::args::ClassCycleReportArgs;
use crate::class_cycle_report::cli::render::print_class_cycle_report;
use crate::class_cycle_report::usecase::{
    ClassCyclePolicyOptions, analyze_class_cycles, collect_class_inheritance_edges,
    evaluate_class_cycle_policy,
};
use paredit_core_cli::shared::{analyze_files, note_partial_file_failures, total_file_failure};

pub fn class_cycle_report(args: ClassCycleReportArgs) -> CommandResult {
    // A file that will not parse is reported, not fatal — see `query find`.
    let analysis = analyze_files(&args.files, args.dialect, |_file, dialect, tree, _| {
        collect_class_inheritance_edges(dialect, tree)
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let edges: Vec<(String, String)> = analysis.succeeded.into_iter().flatten().collect();

    let summary = analyze_class_cycles(&edges);
    let policy =
        evaluate_class_cycle_policy(ClassCyclePolicyOptions::new(args.fail_on_cycle), &summary);
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_class_cycle_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "class-cycle-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
