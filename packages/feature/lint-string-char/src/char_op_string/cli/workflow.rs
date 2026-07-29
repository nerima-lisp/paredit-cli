use paredit_core_cli::CommandResult;

use crate::char_op_string::cli::args::CharOpStringReportArgs;
use crate::char_op_string::cli::render::print_char_op_string_report;
use crate::char_op_string::usecase::{
    CharOpStringPolicyOptions, collect_char_op_strings, evaluate_char_op_string_policy,
    summarize_char_op_strings,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn char_op_string_report(args: CharOpStringReportArgs) -> CommandResult {
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
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "char-op-string-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
