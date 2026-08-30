use paredit_core_cli::{CliResult, CommandResult};

use paredit_core_cli::shared::{
    analyze_files, expand_input_files, note_partial_file_failures, total_file_failure,
};

use crate::read_conditional_report::cli::args::ReadConditionalReportArgs;
use crate::read_conditional_report::cli::render::print_conditional_report;
use crate::read_conditional_report::usecase::{
    build_read_conditional_report, evaluate_fail_on_conditional_policy,
};

pub fn read_conditional_report(args: ReadConditionalReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let analysis = analyze_files(&files, args.dialect, |file, dialect, tree, _| {
        CliResult::Ok(build_read_conditional_report(file, dialect, tree))
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

    let policy = evaluate_fail_on_conditional_policy(args.fail_on_conditional, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_conditional_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect read-conditionals policy failed: {message}"
        )));
    }

    Ok(())
}
