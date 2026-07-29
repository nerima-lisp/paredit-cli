use paredit_core_cli::CommandResult;

use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::unreachable_expression_report::cli::args::UnreachableExpressionReportArgs;
use crate::unreachable_expression_report::cli::render::print_unreachable_report;
use crate::unreachable_expression_report::usecase::{
    build_unreachable_expression_report, evaluate_fail_on_unreachable_policy,
};

pub fn unreachable_expression_report(args: UnreachableExpressionReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_unreachable_expression_report(file, dialect, &tree));
    }

    let policy = evaluate_fail_on_unreachable_policy(args.fail_on_unreachable, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_unreachable_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect unreachable-expressions policy failed: {message}"
        )));
    }

    Ok(())
}
