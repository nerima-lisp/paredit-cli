use paredit_core_cli::CommandResult;

use crate::ftype_values_arity_mismatch::cli::args::FtypeValuesArityMismatchReportArgs;
use crate::ftype_values_arity_mismatch::cli::render::print_ftype_values_arity_mismatch_report;
use crate::ftype_values_arity_mismatch::usecase::{
    build_ftype_values_arity_mismatch_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn ftype_values_arity_mismatch_report(
    args: FtypeValuesArityMismatchReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_ftype_values_arity_mismatch_report(
            file, dialect, &tree,
        )?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_ftype_values_arity_mismatch_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "ftype-values-arity-mismatch-report policy failed: {message}"
        )));
    }

    Ok(())
}
