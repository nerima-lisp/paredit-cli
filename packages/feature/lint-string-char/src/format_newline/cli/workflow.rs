use paredit_core_cli::CommandResult;

use crate::format_newline::cli::args::FormatNewlineReportArgs;
use crate::format_newline::cli::render::print_format_newline_report;
use crate::format_newline::usecase::{
    build_format_newline_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn format_newline_report(args: FormatNewlineReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_format_newline_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_format_newline_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "format-newline-report policy failed: {message}"
        )));
    }

    Ok(())
}
