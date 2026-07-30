use paredit_core_cli::CommandResult;

use crate::duplicate_parameters::cli::args::DuplicateParameterReportArgs;
use crate::duplicate_parameters::cli::render::print_duplicate_parameter_report;
use crate::duplicate_parameters::usecase::{
    build_duplicate_parameter_report, evaluate_fail_on_duplicate_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn duplicate_parameter_report(args: DuplicateParameterReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_duplicate_parameter_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_duplicate_policy(args.fail_on_duplicate, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_duplicate_parameter_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "duplicate-parameter-report policy failed: {message}"
        )));
    }

    Ok(())
}
