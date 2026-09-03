use paredit_core_cli::CommandResult;

use crate::atom_swap_with_side_effect::cli::args::AtomSwapWithSideEffectReportArgs;
use crate::atom_swap_with_side_effect::cli::render::print_atom_swap_with_side_effect_report;
use crate::atom_swap_with_side_effect::usecase::{
    build_atom_swap_with_side_effect_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn atom_swap_with_side_effect_report(args: AtomSwapWithSideEffectReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_atom_swap_with_side_effect_report(
            file, dialect, &tree,
        )?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_atom_swap_with_side_effect_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "atom-swap-with-side-effect-report policy failed: {message}"
        )));
    }

    Ok(())
}
