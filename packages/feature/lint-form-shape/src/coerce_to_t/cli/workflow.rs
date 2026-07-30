use paredit_core_cli::CommandResult;

use crate::coerce_to_t::cli::args::CoerceToTReportArgs;
use crate::coerce_to_t::cli::render::print_coerce_to_t_report;
use crate::coerce_to_t::usecase::{build_coerce_to_t_report, evaluate_fail_on_violation_policy};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn coerce_to_t_report(args: CoerceToTReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_coerce_to_t_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_coerce_to_t_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "coerce-to-t-report policy failed: {message}"
        )));
    }

    Ok(())
}
