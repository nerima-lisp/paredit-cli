use paredit_core_cli::CommandResult;

use crate::explicit_step_delta::cli::args::ExplicitStepDeltaReportArgs;
use crate::explicit_step_delta::cli::render::print_explicit_step_delta_report;
use crate::explicit_step_delta::usecase::{
    build_explicit_step_delta_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn explicit_step_delta_report(args: ExplicitStepDeltaReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_explicit_step_delta_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_explicit_step_delta_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "explicit-step-delta-report policy failed: {message}"
        )));
    }

    Ok(())
}
