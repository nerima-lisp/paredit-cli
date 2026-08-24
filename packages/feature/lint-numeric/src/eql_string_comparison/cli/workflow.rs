use paredit_core_cli::{CliResult, CommandResult};

use crate::eql_string_comparison::cli::args::EqlStringComparisonReportArgs;
use crate::eql_string_comparison::cli::render::print_eql_string_comparison_report;
use crate::eql_string_comparison::usecase::{
    build_eql_string_comparison_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{
    analyze_files_raw, note_partial_file_failures, read_input_dialect_and_tree, total_file_failure,
};

pub fn eql_string_comparison_report(args: EqlStringComparisonReportArgs) -> CommandResult {
    let analysis = analyze_files_raw(&args.files, |file| {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        CliResult::Ok(build_eql_string_comparison_report(file, dialect, &tree)?)
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_eql_string_comparison_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "eql-string-comparison-report policy failed: {message}"
        )));
    }

    Ok(())
}
