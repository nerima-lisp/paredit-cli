use paredit_core_cli::CommandResult;

use crate::equality_arity::cli::args::EqualityArityReportArgs;
use crate::equality_arity::cli::render::print_equality_arity_report;
use crate::equality_arity::usecase::{
    build_equality_arity_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn equality_arity_report(args: EqualityArityReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_equality_arity_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_equality_arity_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "equality-arity-report policy failed: {message}"
        )));
    }

    Ok(())
}
