use paredit_core_cli::CommandResult;

use crate::format_percent_ampersand_adjacent_redundancy::cli::args::FormatPercentAmpersandAdjacentRedundancyReportArgs;
use crate::format_percent_ampersand_adjacent_redundancy::cli::render::print_format_percent_ampersand_adjacent_redundancy_report;
use crate::format_percent_ampersand_adjacent_redundancy::usecase::{
    build_format_percent_ampersand_adjacent_redundancy_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn format_percent_ampersand_adjacent_redundancy_report(
    args: FormatPercentAmpersandAdjacentRedundancyReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_format_percent_ampersand_adjacent_redundancy_report(
            file, dialect, &tree,
        )?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_format_percent_ampersand_adjacent_redundancy_report(
        &reports,
        &policy,
        args.output,
        args.verbosity,
    )?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "format-percent-ampersand-adjacent-redundancy-report policy failed: {message}"
        )));
    }

    Ok(())
}
