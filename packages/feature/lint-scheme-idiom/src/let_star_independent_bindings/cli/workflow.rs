use paredit_core_cli::CommandResult;

use crate::let_star_independent_bindings::cli::args::LetStarIndependentBindingsReportArgs;
use crate::let_star_independent_bindings::cli::render::print_let_star_independent_bindings_report;
use crate::let_star_independent_bindings::usecase::{
    build_let_star_independent_bindings_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn let_star_independent_bindings_report(
    args: LetStarIndependentBindingsReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_let_star_independent_bindings_report(
            file, dialect, &tree,
        )?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_let_star_independent_bindings_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "scheme-let-star-independent-bindings-report policy failed: {message}"
        )));
    }

    Ok(())
}
