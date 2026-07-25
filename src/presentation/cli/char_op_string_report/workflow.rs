use anyhow::Result;

use crate::application::usecase::char_op_string_report::{
    CharOpStringPolicyOptions, collect_char_op_strings, evaluate_char_op_string_policy,
    summarize_char_op_strings,
};
use crate::presentation::cli::char_op_string_report::args::CharOpStringReportArgs;
use crate::presentation::cli::char_op_string_report::render::print_char_op_string_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn char_op_string_report(
    args: CharOpStringReportArgs,
) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut char_call_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_call_count, file_violations) = collect_char_op_strings(file, dialect, &tree)?;
        char_call_count += file_call_count;
        violations.extend(file_violations);
    }

    let summary = summarize_char_op_strings(char_call_count, violations);
    let policy = evaluate_char_op_string_policy(
        CharOpStringPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_char_op_string_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "char-op-string-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
