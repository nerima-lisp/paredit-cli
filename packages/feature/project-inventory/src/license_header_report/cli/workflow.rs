use paredit_core_cli::CommandResult;

use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::license_header_report::cli::args::LicenseHeaderReportArgs;
use crate::license_header_report::cli::render::print_license_header_report;
use crate::license_header_report::usecase::{
    apply_header_consistency, build_license_header_report, evaluate_fail_on_missing_header_policy,
};

pub fn license_header_report(args: LicenseHeaderReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_license_header_report(file, dialect, &tree));
    }

    apply_header_consistency(&mut reports);

    let policy = evaluate_fail_on_missing_header_policy(args.fail_on_missing_header, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_license_header_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect license-headers policy failed: {message}"
        )));
    }

    Ok(())
}
