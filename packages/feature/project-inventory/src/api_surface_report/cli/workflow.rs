use paredit_core_cli::CommandResult;

use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::api_surface_report::cli::args::ApiSurfaceReportArgs;
use crate::api_surface_report::cli::render::print_undefined_export_report;
use crate::api_surface_report::usecase::{
    build_api_surface_report, evaluate_fail_on_undefined_export_policy,
};

pub fn api_surface_report(args: ApiSurfaceReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_api_surface_report(file, dialect, &tree));
    }

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
