use paredit_core_cli::{CliResult, CommandResult};

use crate::atom_swap_with_side_effect::cli::args::AtomSwapWithSideEffectReportArgs;
use crate::atom_swap_with_side_effect::cli::render::print_atom_swap_with_side_effect_report;
use crate::atom_swap_with_side_effect::usecase::{
    build_atom_swap_with_side_effect_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{
    analyze_files_raw, expand_input_files, note_partial_file_failures, read_input_dialect_and_tree,
    total_file_failure,
};

pub fn atom_swap_with_side_effect_report(args: AtomSwapWithSideEffectReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let analysis = analyze_files_raw(&files, |file| {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        CliResult::Ok(build_atom_swap_with_side_effect_report(
            file, dialect, &tree,
        )?)
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

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
