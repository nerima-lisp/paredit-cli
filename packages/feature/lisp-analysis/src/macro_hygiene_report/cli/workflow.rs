use paredit_core_cli::{CliResult, CommandResult};

use paredit_core_cli::shared::{
    analyze_files, expand_input_files, note_partial_file_failures, total_file_failure,
};

use crate::macro_hygiene_report::cli::args::MacroHygieneReportArgs;
use crate::macro_hygiene_report::cli::render::print_risk_report;
use crate::macro_hygiene_report::usecase::{
    build_macro_hygiene_report, evaluate_fail_on_risk_policy,
};

pub fn macro_hygiene_report(args: MacroHygieneReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let analysis = analyze_files(&files, args.dialect, |file, dialect, tree, _| {
        CliResult::Ok(build_macro_hygiene_report(file, dialect, tree))
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

    let policy = evaluate_fail_on_risk_policy(args.fail_on_risk, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_risk_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect macro-hygiene policy failed: {message}"
        )));
    }

    Ok(())
}
