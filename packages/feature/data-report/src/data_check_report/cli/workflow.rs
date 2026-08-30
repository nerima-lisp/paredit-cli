use paredit_core_cli::{CliResult, CommandResult};

use paredit_core_cli::shared::{
    analyze_files, expand_input_files, note_partial_file_failures, total_file_failure,
};

use crate::data_check_report::cli::args::DataCheckReportArgs;
use crate::data_check_report::cli::render::print_data_check_report;
use crate::data_check_report::usecase::{
    build_data_check_report, detect_data_format, evaluate_fail_on_finding_policy,
};

pub fn data_check_report(args: DataCheckReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let analysis = analyze_files(&files, args.dialect, |file, dialect, tree, _| {
        // An explicit `--format` always wins; otherwise the file's path and
        // content decide which detectors run on top of the baseline checks.
        let format = args
            .format
            .map_or_else(|| detect_data_format(file, dialect, tree), Into::into);
        CliResult::Ok(build_data_check_report(file, dialect, tree, format))
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

    let policy = evaluate_fail_on_finding_policy(args.fail_on_finding, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_data_check_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect data-check policy failed: {message}"
        )));
    }

    Ok(())
}
