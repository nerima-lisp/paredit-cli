use super::args::PackageBoundaryReportArgs;
use super::render::print_package_boundary_report;
use crate::package_boundary_report::usecase::{
    PackageBoundaryPolicyOptions, build_package_boundary_report, evaluate_package_boundary_policy,
};
use paredit_core_cli::CliResult;
use paredit_core_cli::CommandResult;
use paredit_core_cli::shared::expand_input_files;
use paredit_core_cli::shared::{analyze_files, note_partial_file_failures, total_file_failure};

pub fn package_boundary_report(args: PackageBoundaryReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;
    let analysis = analyze_files(&files, args.dialect, |file, dialect, tree, _| {
        CliResult::Ok(build_package_boundary_report(file.clone(), dialect, tree)?)
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

    let policy = evaluate_package_boundary_policy(
        PackageBoundaryPolicyOptions::new(args.fail_on_violation),
        &reports,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_package_boundary_report(&reports, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "package-boundary-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
