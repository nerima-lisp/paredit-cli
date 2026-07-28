use anyhow::Result;

use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::line_metrics_report::cli::args::LineMetricsReportArgs;
use crate::line_metrics_report::cli::render::print_line_metrics_report;
use crate::line_metrics_report::usecase::{
    build_line_metrics_report, evaluate_line_metrics_policy,
};

pub fn line_metrics_report(args: LineMetricsReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;
    let thresholds = args.thresholds();

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_line_metrics_report(file, dialect, &tree, thresholds));
    }

    let policy = evaluate_line_metrics_policy(args.fail_on_overflow, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_line_metrics_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect line-metrics policy failed: {message}"
        )));
    }

    Ok(())
}
