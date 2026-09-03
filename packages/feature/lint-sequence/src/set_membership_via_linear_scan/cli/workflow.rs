use paredit_core_cli::CommandResult;

use crate::set_membership_via_linear_scan::cli::args::SetMembershipViaLinearScanReportArgs;
use crate::set_membership_via_linear_scan::cli::render::print_set_membership_via_linear_scan_report;
use crate::set_membership_via_linear_scan::usecase::{
    collect_linear_scans, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn set_membership_via_linear_scan_report(
    args: SetMembershipViaLinearScanReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(collect_linear_scans(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_set_membership_via_linear_scan_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "set-membership-via-linear-scan-report policy failed: {message}"
        )));
    }

    Ok(())
}
