use paredit_core_cli::{CliResult, CommandResult};

use paredit_core_cli::shared::{
    analyze_files, expand_input_files, note_partial_file_failures, total_file_failure,
};

use crate::macro_expansion_report::cli::args::MacroExpansionReportArgs;
use crate::macro_expansion_report::cli::render::print_declined_report;
use crate::macro_expansion_report::usecase::{
    build_macro_expansion_report, evaluate_fail_on_declined_policy,
};

pub fn macro_expansion_report(args: MacroExpansionReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let analysis = analyze_files(&files, args.dialect, |file, dialect, tree, _| {
        CliResult::Ok(build_macro_expansion_report(file, dialect, tree))
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

    let policy = evaluate_fail_on_declined_policy(args.fail_on_declined, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_declined_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect macro-expansion policy failed: {message}"
        )));
    }

    Ok(())
}
