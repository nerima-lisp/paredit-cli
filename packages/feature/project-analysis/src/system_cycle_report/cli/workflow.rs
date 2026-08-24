use paredit_core_cli::CommandResult;

use crate::error::ProjectAnalysisResult;
use crate::system_cycle_report::cli::args::SystemCycleReportArgs;
use crate::system_cycle_report::cli::render::print_system_cycle_report;
use crate::system_cycle_report::usecase::{
    SystemCyclePolicyOptions, analyze_system_cycles, build_system_dependency_edges,
    evaluate_system_cycle_policy,
};
use paredit_core_cli::shared::{analyze_files, note_partial_file_failures, total_file_failure};

pub fn system_cycle_report(args: SystemCycleReportArgs) -> CommandResult {
    // A file that will not parse is reported, not fatal — see `query find`.
    let analysis = analyze_files(&args.files, args.dialect, |_file, dialect, tree, _| {
        ProjectAnalysisResult::Ok(build_system_dependency_edges(tree, dialect)?)
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let edges: Vec<(String, String)> = analysis.succeeded.into_iter().flatten().collect();

    let summary = analyze_system_cycles(&edges);
    let policy =
        evaluate_system_cycle_policy(SystemCyclePolicyOptions::new(args.fail_on_cycle), &summary);
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_system_cycle_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "system-cycle-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
