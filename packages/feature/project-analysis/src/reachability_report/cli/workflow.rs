use paredit_core_cli::CommandResult;

use crate::call_graph_report::usecase::{CallGraphReportSource, build_call_graph_report};
use crate::reachability_report::cli::args::ReachabilityReportArgs;
use crate::reachability_report::cli::render::print_reachability_report;
use crate::reachability_report::usecase::{
    ReachabilityReportPolicyOptions, analyze_reachability, evaluate_reachability_policy,
};
use paredit_core_cli::shared::read_input_dialect_and_tree;

pub fn reachability_report(args: ReachabilityReportArgs) -> CommandResult {
    let mut sources = Vec::with_capacity(args.files.len());

    for file in &args.files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        sources.push(CallGraphReportSource {
            path: file.clone(),
            dialect,
            tree,
        });
    }

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
