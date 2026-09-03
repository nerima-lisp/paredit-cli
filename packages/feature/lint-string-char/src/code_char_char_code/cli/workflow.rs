use paredit_core_cli::CommandResult;

use crate::code_char_char_code::cli::args::CodeCharCharCodeReportArgs;
use crate::code_char_char_code::cli::render::print_code_char_char_code_report;
use crate::code_char_char_code::usecase::{
    build_code_char_char_code_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn code_char_char_code_report(args: CodeCharCharCodeReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_code_char_char_code_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_code_char_char_code_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "code-char-char-code-report policy failed: {message}"
        )));
    }

    Ok(())
}
