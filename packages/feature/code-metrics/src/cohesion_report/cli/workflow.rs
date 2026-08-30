use paredit_core_cli::{CliResult, CommandResult};

use paredit_core_cli::shared::{
    analyze_files, expand_input_files, note_partial_file_failures, total_file_failure,
};

use crate::cohesion_report::cli::args::CohesionReportArgs;
use crate::cohesion_report::cli::render::print_isolated_report;
use crate::cohesion_report::usecase::{build_cohesion_report, evaluate_fail_on_isolated_policy};

pub fn cohesion_report(args: CohesionReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let analysis = analyze_files(&files, args.dialect, |file, dialect, tree, _| {
        CliResult::Ok(build_cohesion_report(file, dialect, tree))
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

    let policy = evaluate_fail_on_isolated_policy(args.fail_on_isolated, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_isolated_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect cohesion policy failed: {message}"
        )));
    }

    Ok(())
}
