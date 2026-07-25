use anyhow::Result;

use crate::application::usecase::format_to_string_report::{
    FormatToStringPolicyOptions, collect_format_to_strings, evaluate_format_to_string_policy,
    summarize_format_to_strings,
};
use crate::presentation::cli::format_to_string_report::args::FormatToStringReportArgs;
use crate::presentation::cli::format_to_string_report::render::print_format_to_string_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn format_to_string_report(
    args: FormatToStringReportArgs,
) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut format_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_format_to_strings(file, dialect, &tree)?;
        format_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_format_to_strings(format_form_count, violations);
    let policy = evaluate_format_to_string_policy(
        FormatToStringPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_format_to_string_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "format-to-string-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
