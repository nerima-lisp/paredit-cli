use anyhow::Result;

use crate::format_newline::cli::args::FormatNewlineReportArgs;
use crate::format_newline::cli::render::print_format_newline_report;
use crate::format_newline::usecase::{
    FormatNewlinePolicyOptions, collect_format_newlines, evaluate_format_newline_policy,
    summarize_format_newlines,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn format_newline_report(args: FormatNewlineReportArgs) -> Result<()> {
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
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "format-newline-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
