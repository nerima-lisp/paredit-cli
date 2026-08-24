use paredit_core_cli::{CliResult, CommandResult};

use paredit_core_cli::shared::{
    analyze_files, expand_input_files, note_partial_file_failures, total_file_failure,
};

use crate::restart_report::cli::args::RestartReportArgs;
use crate::restart_report::cli::render::print_unpaired_report;
use crate::restart_report::usecase::{build_restart_report, evaluate_fail_on_unpaired_policy};

pub fn restart_report(args: RestartReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let analysis = analyze_files(&files, args.dialect, |file, dialect, tree, _| {
        CliResult::Ok(build_restart_report(file, dialect, tree))
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

    let policy = evaluate_fail_on_unpaired_policy(args.fail_on_unpaired, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_unpaired_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect restarts policy failed: {message}"
        )));
    }

    Ok(())
}
