use paredit_core_cli::{CliResult, CommandResult};

use paredit_core_cli::shared::{
    analyze_files, expand_input_files, note_partial_file_failures, total_file_failure,
};

use crate::api_surface_report::cli::args::ApiSurfaceReportArgs;
use crate::api_surface_report::cli::render::print_undefined_export_report;
use crate::api_surface_report::usecase::{
    build_api_surface_report, evaluate_fail_on_undefined_export_policy,
};

pub fn api_surface_report(args: ApiSurfaceReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let analysis = analyze_files(&files, args.dialect, |file, dialect, tree, _| {
        CliResult::Ok(build_api_surface_report(file, dialect, tree))
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

    let policy = evaluate_fail_on_undefined_export_policy(args.fail_on_undefined_export, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_undefined_export_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect api-surface policy failed: {message}"
        )));
    }

    Ok(())
}
