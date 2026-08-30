use paredit_core_cli::{CliResult, CommandResult};

use paredit_core_cli::shared::{
    analyze_files, expand_input_files, note_partial_file_failures, total_file_failure,
};

use crate::magic_number_report::cli::args::MagicNumberReportArgs;
use crate::magic_number_report::cli::render::print_magic_number_report;
use crate::magic_number_report::usecase::{
    build_magic_number_report, evaluate_fail_on_magic_number_policy,
};

pub fn magic_number_report(args: MagicNumberReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let analysis = analyze_files(&files, args.dialect, |file, dialect, tree, _| {
        CliResult::Ok(build_magic_number_report(file, dialect, tree))
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

    let policy = evaluate_fail_on_magic_number_policy(args.fail_on_magic_number, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_magic_number_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect magic-numbers policy failed: {message}"
        )));
    }

    Ok(())
}
