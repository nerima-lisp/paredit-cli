use paredit_core_cli::CommandResult;

use crate::char_op_string::cli::args::CharOpStringReportArgs;
use crate::char_op_string::cli::render::print_char_op_string_report;
use crate::char_op_string::usecase::{
    build_char_op_string_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn char_op_string_report(args: CharOpStringReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_char_op_string_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_char_op_string_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "char-op-string-report policy failed: {message}"
        )));
    }

    Ok(())
}
