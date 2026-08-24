use paredit_core_cli::{CliResult, CommandResult};

use paredit_core_cli::shared::{
    analyze_files, expand_input_files, note_partial_file_failures, total_file_failure,
};

use crate::debt_score_report::cli::args::DebtScoreReportArgs;
use crate::debt_score_report::cli::render::print_debt_report;
use crate::debt_score_report::usecase::{build_debt_score_report, evaluate_fail_on_debt_policy};

pub fn debt_score_report(args: DebtScoreReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let analysis = analyze_files(&files, args.dialect, |file, dialect, tree, _| {
        CliResult::Ok(build_debt_score_report(file, dialect, tree))
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

    let policy = evaluate_fail_on_debt_policy(args.fail_on_debt, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_debt_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect debt-score policy failed: {message}"
        )));
    }

    Ok(())
}
