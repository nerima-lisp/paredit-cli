use paredit_core_cli::CommandResult;

use crate::nil_comparison::cli::args::NilComparisonReportArgs;
use crate::nil_comparison::cli::render::print_nil_comparison_report;
use crate::nil_comparison::usecase::{
    build_nil_comparison_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn nil_comparison_report(args: NilComparisonReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_nil_comparison_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_nil_comparison_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "nil-comparison-report policy failed: {message}"
        )));
    }

    Ok(())
}
