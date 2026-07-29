use paredit_core_cli::CommandResult;

use crate::one_armed_if::cli::args::OneArmedIfReportArgs;
use crate::one_armed_if::cli::render::print_one_armed_if_report;
use crate::one_armed_if::usecase::{build_one_armed_if_report, evaluate_fail_on_violation_policy};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn one_armed_if_report(args: OneArmedIfReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_one_armed_if_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_one_armed_if_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "one-armed-if-report policy failed: {message}"
        )));
    }

    Ok(())
}
