use paredit_core_cli::CommandResult;

use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::debt_score_report::cli::args::DebtScoreReportArgs;
use crate::debt_score_report::cli::render::print_debt_report;
use crate::debt_score_report::usecase::{build_debt_score_report, evaluate_fail_on_debt_policy};

pub fn debt_score_report(args: DebtScoreReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_debt_score_report(file, dialect, &tree));
    }

    let policy = evaluate_fail_on_debt_policy(args.fail_on_debt, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_debt_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect debt-score policy failed: {message}"
        )));
    }

    Ok(())
}
