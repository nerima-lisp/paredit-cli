use anyhow::Result;

use crate::application::usecase::call_graph_report::{
    CallGraphReportSource, build_call_graph_report,
};
use crate::application::usecase::reachability_report::{
    ReachabilityReportPolicyOptions, analyze_reachability, evaluate_reachability_policy,
};
use crate::presentation::cli::reachability_report::args::ReachabilityReportArgs;
use crate::presentation::cli::reachability_report::render::print_reachability_report;
use crate::presentation::cli::shared::read_input_dialect_and_tree;

pub(in crate::presentation::cli) fn reachability_report(
    args: ReachabilityReportArgs,
) -> Result<()> {
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
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "reachability-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
