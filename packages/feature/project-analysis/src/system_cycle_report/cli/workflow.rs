use anyhow::Result;

use crate::application::usecase::system_cycle_report::{
    SystemCyclePolicyOptions, analyze_system_cycles, build_system_dependency_edges,
    evaluate_system_cycle_policy,
};
use crate::presentation::cli::shared::read_input_dialect_and_tree;
use crate::presentation::cli::system_cycle_report::args::SystemCycleReportArgs;
use crate::presentation::cli::system_cycle_report::render::print_system_cycle_report;

pub(in crate::presentation::cli) fn system_cycle_report(args: SystemCycleReportArgs) -> Result<()> {
    let mut edges = Vec::new();

    for file in &args.files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        edges.extend(build_system_dependency_edges(&tree, dialect)?);
    }

    let summary = analyze_system_cycles(&edges);
    let policy =
        evaluate_system_cycle_policy(SystemCyclePolicyOptions::new(args.fail_on_cycle), &summary);
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_system_cycle_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "system-cycle-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
