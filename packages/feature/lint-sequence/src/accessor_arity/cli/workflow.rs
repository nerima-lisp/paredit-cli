use paredit_core_cli::CommandResult;

use crate::accessor_arity::cli::args::AccessorArityReportArgs;
use crate::accessor_arity::cli::render::print_accessor_arity_report;
use crate::accessor_arity::usecase::{
    collect_accessor_arity_violations, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn accessor_arity_report(args: AccessorArityReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(collect_accessor_arity_violations(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_accessor_arity_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "accessor-arity-report policy failed: {message}"
        )));
    }

    Ok(())
}
