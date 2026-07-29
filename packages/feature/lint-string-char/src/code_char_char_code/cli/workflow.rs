use paredit_core_cli::CommandResult;

use crate::code_char_char_code::cli::args::CodeCharCharCodeReportArgs;
use crate::code_char_char_code::cli::render::print_code_char_char_code_report;
use crate::code_char_char_code::usecase::{
    CodeCharCharCodePolicyOptions, collect_code_char_char_codes,
    evaluate_code_char_char_code_policy, summarize_code_char_char_codes,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn code_char_char_code_report(args: CodeCharCharCodeReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut code_char_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) =
            collect_code_char_char_codes(file, dialect, &tree)?;
        code_char_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_code_char_char_codes(code_char_form_count, violations);
    let policy = evaluate_code_char_char_code_policy(
        CodeCharCharCodePolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_code_char_char_code_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "code-char-char-code-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
