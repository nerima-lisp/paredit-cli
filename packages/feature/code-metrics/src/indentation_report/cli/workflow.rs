use paredit_core_cli::{CliResult, CommandResult};

use paredit_core_cli::shared::{
    analyze_files, expand_input_files, note_partial_file_failures, total_file_failure,
};

use crate::indentation_report::cli::args::IndentationReportArgs;
use crate::indentation_report::cli::render::print_deviation_report;
use crate::indentation_report::usecase::{
    build_indentation_report, evaluate_fail_on_deviation_policy,
};

pub fn indentation_report(args: IndentationReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let analysis = analyze_files(&files, args.dialect, |file, dialect, tree, _| {
        CliResult::Ok(build_indentation_report(file, dialect, tree))
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

    let policy = evaluate_fail_on_deviation_policy(args.fail_on_deviation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_deviation_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect indentation policy failed: {message}"
        )));
    }

    Ok(())
}
