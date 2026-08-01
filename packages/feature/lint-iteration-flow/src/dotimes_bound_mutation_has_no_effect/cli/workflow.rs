use paredit_core_cli::CommandResult;

use crate::dotimes_bound_mutation_has_no_effect::cli::args::DotimesBoundMutationHasNoEffectReportArgs;
use crate::dotimes_bound_mutation_has_no_effect::cli::render::print_dotimes_bound_mutation_has_no_effect_report;
use crate::dotimes_bound_mutation_has_no_effect::usecase::{
    build_dotimes_bound_mutation_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn dotimes_bound_mutation_has_no_effect_report(
    args: DotimesBoundMutationHasNoEffectReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_dotimes_bound_mutation_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_dotimes_bound_mutation_has_no_effect_report(
        &reports,
        &policy,
        args.output,
        args.verbosity,
    )?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "dotimes-bound-mutation-has-no-effect-report policy failed: {message}"
        )));
    }

    Ok(())
}
