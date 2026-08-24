use paredit_core_cli::CommandResult;

use crate::call_cycle_report::cli::args::CallCycleReportArgs;
use crate::call_cycle_report::cli::render::print_call_cycle_report;
use crate::call_cycle_report::usecase::{
    CallCyclePolicyOptions, analyze_call_cycles, evaluate_call_cycle_policy,
};
use crate::call_graph_report::usecase::{CallGraphReportSource, build_call_graph_report};
use paredit_core_cli::CliResult;
use paredit_core_cli::shared::{analyze_files, note_partial_file_failures, total_file_failure};

pub fn call_cycle_report(args: CallCycleReportArgs) -> CommandResult {
    // A file that will not parse is reported, not fatal — see `query find`.
    let analysis = analyze_files(&args.files, args.dialect, |file, dialect, tree, _| {
        CliResult::Ok(CallGraphReportSource {
            path: file.clone(),
            dialect,
            tree: tree.clone(),
        })
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let sources = analysis.succeeded;

    let report = build_call_graph_report(sources, false, None)?;
    let summary = analyze_call_cycles(&report.files);
    let policy =
        evaluate_call_cycle_policy(CallCyclePolicyOptions::new(args.fail_on_cycle), &summary);
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_call_cycle_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "call-cycle-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
