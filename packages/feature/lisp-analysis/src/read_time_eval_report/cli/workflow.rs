use paredit_core_cli::{CliResult, CommandResult};

use paredit_core_cli::shared::{
    analyze_files, expand_input_files, note_partial_file_failures, total_file_failure,
};

use crate::read_time_eval_report::cli::args::ReadTimeEvalReportArgs;
use crate::read_time_eval_report::cli::render::print_read_eval_report;
use crate::read_time_eval_report::usecase::{
    build_read_time_eval_report, evaluate_fail_on_read_eval_policy,
};

pub fn read_time_eval_report(args: ReadTimeEvalReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let analysis = analyze_files(&files, args.dialect, |file, dialect, tree, _| {
        CliResult::Ok(build_read_time_eval_report(file, dialect, tree))
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

    let policy = evaluate_fail_on_read_eval_policy(args.fail_on_read_eval, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_read_eval_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect read-time-eval policy failed: {message}"
        )));
    }

    Ok(())
}
