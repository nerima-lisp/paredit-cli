use anyhow::Result;

use crate::application::usecase::package_cycle_report::{
    PackageCyclePolicyOptions, analyze_package_cycles, collect_package_dependency_edges,
    evaluate_package_cycle_policy,
};
use crate::presentation::cli::package_cycle_report::args::PackageCycleReportArgs;
use crate::presentation::cli::package_cycle_report::render::print_package_cycle_report;
use crate::presentation::cli::shared::read_input_dialect_and_tree;

pub(in crate::presentation::cli) fn package_cycle_report(
    args: PackageCycleReportArgs,
) -> Result<()> {
    let mut edges = Vec::new();

    for file in &args.files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        edges.extend(collect_package_dependency_edges(dialect, &tree)?);
    }

    let summary = analyze_package_cycles(&edges);
    let policy =
        evaluate_package_cycle_policy(PackageCyclePolicyOptions::new(args.fail_on_cycle), &summary);
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_package_cycle_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "package-cycle-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
