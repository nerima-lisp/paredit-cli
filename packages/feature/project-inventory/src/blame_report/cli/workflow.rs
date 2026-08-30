use paredit_core_cli::{CliResult, CommandResult};

use paredit_core_cli::shared::{
    analyze_files, expand_input_files, note_partial_file_failures, total_file_failure,
};

use crate::blame_report::cli::args::BlameReportArgs;
use crate::blame_report::cli::render::print_blame_report;
use crate::blame_report::usecase::{build_blame_report, evaluate_blame_policy, measure_blame};

pub fn blame_report(args: BlameReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let analysis = analyze_files(&files, args.dialect, |file, dialect, tree, _| {
        // One `git blame` per file, run in the file's own directory so a report
        // spanning several repositories answers for each of them.
        let blame = measure_blame(&file.canonicalize().unwrap_or_else(|_| file.to_path_buf()));
        CliResult::Ok(build_blame_report(file, dialect, tree, &blame))
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

    let policy = evaluate_blame_policy(args.fail_on_unattributed, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_blame_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect blame policy failed: {message}"
        )));
    }

    Ok(())
}
