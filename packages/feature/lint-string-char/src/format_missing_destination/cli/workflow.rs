use paredit_core_cli::CommandResult;

use crate::format_missing_destination::cli::args::FormatMissingDestinationReportArgs;
use crate::format_missing_destination::cli::render::print_format_missing_destination_report;
use crate::format_missing_destination::usecase::{
    build_format_missing_destination_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn format_missing_destination_report(
    args: FormatMissingDestinationReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_format_missing_destination_report(
            file, dialect, &tree,
        )?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_format_missing_destination_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "format-missing-destination-report policy failed: {message}"
        )));
    }

    Ok(())
}
