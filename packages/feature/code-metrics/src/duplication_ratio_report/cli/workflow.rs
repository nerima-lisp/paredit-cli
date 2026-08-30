use paredit_core_cli::{CliResult, CommandResult};

use paredit_core_cli::shared::{
    analyze_files, expand_input_files, note_partial_file_failures, total_file_failure,
};

use crate::duplication_ratio_report::cli::args::DuplicationRatioReportArgs;
use crate::duplication_ratio_report::cli::render::print_duplication_report;
use crate::duplication_ratio_report::usecase::{
    build_duplication_ratio_report, evaluate_fail_on_duplication_policy,
};

pub fn duplication_ratio_report(args: DuplicationRatioReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let analysis = analyze_files(&files, args.dialect, |file, dialect, tree, _| {
        CliResult::Ok(build_duplication_ratio_report(file, dialect, tree))
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

    let policy = evaluate_fail_on_duplication_policy(args.fail_on_duplication, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_duplication_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect duplication-ratio policy failed: {message}"
        )));
    }

    Ok(())
}
