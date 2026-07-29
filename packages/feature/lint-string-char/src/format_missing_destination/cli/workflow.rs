use paredit_core_cli::CommandResult;

use crate::format_missing_destination::cli::args::FormatMissingDestinationReportArgs;
use crate::format_missing_destination::cli::render::print_format_missing_destination_report;
use crate::format_missing_destination::usecase::{
    FormatMissingDestinationPolicyOptions, collect_format_missing_destinations,
    evaluate_format_missing_destination_policy, summarize_format_missing_destinations,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn format_missing_destination_report(
    args: FormatMissingDestinationReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut format_call_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_call_count, file_violations) =
            collect_format_missing_destinations(file, dialect, &tree)?;
        format_call_count += file_call_count;
        violations.extend(file_violations);
    }

    let summary = summarize_format_missing_destinations(format_call_count, violations);
    let policy = evaluate_format_missing_destination_policy(
        FormatMissingDestinationPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_format_missing_destination_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "format-missing-destination-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
