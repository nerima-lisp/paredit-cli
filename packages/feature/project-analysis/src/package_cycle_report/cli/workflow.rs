use paredit_core_cli::CommandResult;

use crate::package_cycle_report::cli::args::PackageCycleReportArgs;
use crate::package_cycle_report::cli::render::print_package_cycle_report;
use crate::package_cycle_report::usecase::{
    PackageCyclePolicyOptions, analyze_package_cycles, collect_package_dependency_edges,
    evaluate_package_cycle_policy,
};
use paredit_core_cli::shared::{analyze_files, note_partial_file_failures, total_file_failure};

pub fn package_cycle_report(args: PackageCycleReportArgs) -> CommandResult {
    // A file that will not parse is reported, not fatal — see `query find`.
    let analysis = analyze_files(&args.files, args.dialect, |_file, dialect, tree, _| {
        collect_package_dependency_edges(dialect, tree)
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let edges: Vec<(String, String)> = analysis.succeeded.into_iter().flatten().collect();

    let summary = analyze_package_cycles(&edges);
    let policy =
        evaluate_package_cycle_policy(PackageCyclePolicyOptions::new(args.fail_on_cycle), &summary);
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_package_cycle_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "package-cycle-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
