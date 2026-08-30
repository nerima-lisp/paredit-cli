use paredit_core_cli::{CliResult, CommandResult};

use paredit_core_cli::shared::{
    analyze_files, expand_input_files, note_partial_file_failures, total_file_failure,
};

use crate::test_map_report::cli::args::TestMapReportArgs;
use crate::test_map_report::cli::render::print_untested_report;
use crate::test_map_report::usecase::{build_test_map_report, evaluate_fail_on_untested_policy};

pub fn test_map_report(args: TestMapReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let analysis = analyze_files(&files, args.dialect, |file, dialect, tree, _| {
        CliResult::Ok(build_test_map_report(file, dialect, tree))
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

    let policy = evaluate_fail_on_untested_policy(args.fail_on_untested, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_untested_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect test-map policy failed: {message}"
        )));
    }

    Ok(())
}
