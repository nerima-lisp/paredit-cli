use paredit_core_cli::{CliError, CommandResult};

use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::api_diff_report::cli::args::{ApiDiffReportArgs, IntendedBump};
use crate::api_diff_report::cli::render::print_api_diff_report;
use crate::api_diff_report::usecase::{
    build_api_diff_report, evaluate_api_diff_policy, read_baseline,
};

pub fn api_diff_report(args: ApiDiffReportArgs) -> CommandResult {
    let document = std::fs::read_to_string(&args.baseline).map_err(CliError::io(format!(
        "read baseline {}",
        args.baseline.display()
    )))?;
    let document: serde_json::Value =
        serde_json::from_str(&document).map_err(|source| CliError::Json {
            context: format!("parse baseline {}", args.baseline.display()),
            source,
        })?;
    let baseline = read_baseline(&document);

    let files = expand_input_files(&args.files, args.dialect)?;
    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_api_diff_report(file, dialect, &tree, &baseline));
    }

    let policy = evaluate_api_diff_policy(args.intended_bump.map(IntendedBump::label), &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_api_diff_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect api-diff policy failed: {message}"
        )));
    }

    Ok(())
}
