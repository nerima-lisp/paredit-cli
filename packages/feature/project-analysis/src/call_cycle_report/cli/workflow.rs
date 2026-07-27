use anyhow::Result;

use crate::call_cycle_report::cli::args::CallCycleReportArgs;
use crate::call_cycle_report::cli::render::print_call_cycle_report;
use crate::call_cycle_report::usecase::{
    CallCyclePolicyOptions, analyze_call_cycles, evaluate_call_cycle_policy,
};
use crate::call_graph_report::usecase::{CallGraphReportSource, build_call_graph_report};
use paredit_core_cli::shared::read_input_dialect_and_tree;

pub fn call_cycle_report(args: CallCycleReportArgs) -> Result<()> {
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
