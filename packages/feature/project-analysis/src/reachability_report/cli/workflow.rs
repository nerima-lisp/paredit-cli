use paredit_core_cli::CommandResult;

use crate::call_graph_report::usecase::{CallGraphReportSource, build_call_graph_report};
use crate::reachability_report::cli::args::ReachabilityReportArgs;
use crate::reachability_report::cli::render::print_reachability_report;
use crate::reachability_report::usecase::{
    ReachabilityReportPolicyOptions, analyze_reachability, evaluate_reachability_policy,
};
use paredit_core_cli::CliResult;
use paredit_core_cli::shared::{analyze_files, note_partial_file_failures, total_file_failure};

pub fn reachability_report(args: ReachabilityReportArgs) -> CommandResult {
    // A file that will not parse is reported, not fatal — see `query find`.
    let analysis = analyze_files(&args.files, args.dialect, |file, dialect, tree, _| {
        CliResult::Ok(CallGraphReportSource {
            path: file.to_path_buf(),
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
    let summary = analyze_reachability(&report.files);
    let policy = evaluate_reachability_policy(
        ReachabilityReportPolicyOptions::new(args.fail_on_unreachable),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_reachability_report(&report.files, &summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "reachability-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
