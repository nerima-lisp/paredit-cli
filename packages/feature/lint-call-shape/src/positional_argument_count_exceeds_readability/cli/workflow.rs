use paredit_core_cli::CommandResult;
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::positional_argument_count_exceeds_readability::cli::args::PositionalArgumentCountReportArgs;
use crate::positional_argument_count_exceeds_readability::cli::render::print_positional_argument_count_report;
use crate::positional_argument_count_exceeds_readability::usecase::{
    build_positional_argument_count_report, evaluate_fail_on_violation_policy,
};

pub fn positional_argument_count_report(args: PositionalArgumentCountReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_positional_argument_count_report(
            file, dialect, &tree,
        )?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_positional_argument_count_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "positional-argument-count-exceeds-readability-report policy failed: {message}"
        )));
    }

    Ok(())
}
