use paredit_core_cli::{CliResult, CommandResult};

use paredit_core_cli::shared::{
    analyze_files, expand_input_files, note_partial_file_failures, total_file_failure,
};

use crate::format_directive_report::cli::args::FormatDirectiveReportArgs;
use crate::format_directive_report::cli::render::print_mismatch_report;
use crate::format_directive_report::usecase::{
    build_format_directive_report, evaluate_fail_on_mismatch_policy,
};

pub fn format_directive_report(args: FormatDirectiveReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let analysis = analyze_files(&files, args.dialect, |file, dialect, tree, _| {
        CliResult::Ok(build_format_directive_report(file, dialect, tree))
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

    let policy = evaluate_fail_on_mismatch_policy(args.fail_on_mismatch, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_mismatch_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect format-directives policy failed: {message}"
        )));
    }

    Ok(())
}
