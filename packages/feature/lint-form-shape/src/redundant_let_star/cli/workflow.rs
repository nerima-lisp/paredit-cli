use paredit_core_cli::CommandResult;

use crate::redundant_let_star::cli::args::RedundantLetStarReportArgs;
use crate::redundant_let_star::cli::render::print_redundant_let_star_report;
use crate::redundant_let_star::usecase::{
    build_redundant_let_star_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn redundant_let_star_report(args: RedundantLetStarReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_redundant_let_star_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_redundant_let_star_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "redundant-let-star-report policy failed: {message}"
        )));
    }

    Ok(())
}
