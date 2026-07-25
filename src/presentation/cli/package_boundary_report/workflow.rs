use super::super::*;
use super::args::PackageBoundaryReportArgs;
use super::render::print_package_boundary_report;
use crate::application::usecase::package_boundary_report::{
    PackageBoundaryPolicyOptions, build_package_boundary_report, evaluate_package_boundary_policy,
};
use crate::presentation::cli::shared::expand_input_files;

pub(in crate::presentation::cli) fn package_boundary_report(
    args: PackageBoundaryReportArgs,
) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;
    let mut reports = Vec::with_capacity(files.len());

    for file in &files {
        let (_input, dialect, tree) =
            read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_package_boundary_report(file.clone(), dialect, &tree)?);
    }

    let policy = evaluate_package_boundary_policy(
        PackageBoundaryPolicyOptions::new(args.fail_on_violation),
        &reports,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_package_boundary_report(&reports, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "package-boundary-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
