use anyhow::Result;

use crate::application::usecase::format_newline_report::{
    FormatNewlinePolicyOptions, collect_format_newlines, evaluate_format_newline_policy,
    summarize_format_newlines,
};
use crate::presentation::cli::format_newline_report::args::FormatNewlineReportArgs;
use crate::presentation::cli::format_newline_report::render::print_format_newline_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn format_newline_report(
    args: FormatNewlineReportArgs,
) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut format_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_format_newlines(file, dialect, &tree)?;
        format_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_format_newlines(format_form_count, violations);
    let policy = evaluate_format_newline_policy(
        FormatNewlinePolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_format_newline_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "format-newline-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
