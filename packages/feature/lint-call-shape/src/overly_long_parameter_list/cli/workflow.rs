use paredit_core_cli::CommandResult;
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::overly_long_parameter_list::cli::args::OverlyLongParameterListReportArgs;
use crate::overly_long_parameter_list::cli::render::print_overly_long_parameter_list_report;
use crate::overly_long_parameter_list::usecase::{
    build_overly_long_parameter_list_report, evaluate_fail_on_violation_policy,
};

pub fn overly_long_parameter_list_report(args: OverlyLongParameterListReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_overly_long_parameter_list_report(
            file, dialect, &tree,
        )?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_overly_long_parameter_list_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "overly-long-parameter-list-report policy failed: {message}"
        )));
    }

    Ok(())
}
