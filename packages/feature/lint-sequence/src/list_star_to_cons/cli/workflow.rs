use paredit_core_cli::CommandResult;

use crate::list_star_to_cons::cli::args::ListStarToConsReportArgs;
use crate::list_star_to_cons::cli::render::print_list_star_to_cons_report;
use crate::list_star_to_cons::usecase::{
    build_list_star_to_cons_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn list_star_to_cons_report(args: ListStarToConsReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_list_star_to_cons_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_list_star_to_cons_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "list-star-to-cons-report policy failed: {message}"
        )));
    }

    Ok(())
}
