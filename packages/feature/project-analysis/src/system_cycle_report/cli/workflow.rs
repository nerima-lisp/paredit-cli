use anyhow::Result;

use crate::system_cycle_report::cli::args::SystemCycleReportArgs;
use crate::system_cycle_report::cli::render::print_system_cycle_report;
use crate::system_cycle_report::usecase::{
    SystemCyclePolicyOptions, analyze_system_cycles, build_system_dependency_edges,
    evaluate_system_cycle_policy,
};
use paredit_core_cli::shared::read_input_dialect_and_tree;

pub fn system_cycle_report(args: SystemCycleReportArgs) -> Result<()> {
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
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "system-cycle-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
