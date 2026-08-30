use paredit_core_cli::CommandResult;

use crate::struct_cycle_report::cli::args::StructCycleReportArgs;
use crate::struct_cycle_report::cli::render::print_struct_cycle_report;
use crate::struct_cycle_report::usecase::{
    StructCyclePolicyOptions, analyze_struct_cycles, collect_struct_inheritance_edges,
    evaluate_struct_cycle_policy,
};
use paredit_core_cli::shared::{analyze_files, note_partial_file_failures, total_file_failure};

pub fn struct_cycle_report(args: StructCycleReportArgs) -> CommandResult {
    // A file that will not parse is reported, not fatal — see `query find`.
    let analysis = analyze_files(&args.files, args.dialect, |_file, dialect, tree, _| {
        collect_struct_inheritance_edges(dialect, tree)
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let edges: Vec<(String, String)> = analysis.succeeded.into_iter().flatten().collect();

    let summary = analyze_struct_cycles(&edges);
    let policy =
        evaluate_struct_cycle_policy(StructCyclePolicyOptions::new(args.fail_on_cycle), &summary);
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_struct_cycle_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "struct-cycle-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
