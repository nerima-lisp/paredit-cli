use paredit_core_cli::{CliResult, CommandResult};

use paredit_core_cli::shared::{
    analyze_files, expand_input_files, note_partial_file_failures, total_file_failure,
};

use crate::line_metrics_report::cli::args::LineMetricsReportArgs;
use crate::line_metrics_report::cli::render::print_line_metrics_report;
use crate::line_metrics_report::usecase::{
    build_line_metrics_report, evaluate_line_metrics_policy,
};

pub fn line_metrics_report(args: LineMetricsReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;
    let thresholds = args.thresholds();

    let analysis = analyze_files(&files, args.dialect, |file, dialect, tree, _| {
        CliResult::Ok(build_line_metrics_report(file, dialect, tree, thresholds))
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

    let policy = evaluate_line_metrics_policy(args.fail_on_overflow, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_line_metrics_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect line-metrics policy failed: {message}"
        )));
    }

    Ok(())
}
