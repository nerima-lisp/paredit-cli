use paredit_core_cli::CommandResult;

use crate::redundant_into_empty_collection::cli::args::RedundantIntoEmptyCollectionReportArgs;
use crate::redundant_into_empty_collection::cli::render::print_redundant_into_empty_collection_report;
use crate::redundant_into_empty_collection::usecase::{
    collect_redundant_intos, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn redundant_into_empty_collection_report(
    args: RedundantIntoEmptyCollectionReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(collect_redundant_intos(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_redundant_into_empty_collection_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "redundant-into-empty-collection-report policy failed: {message}"
        )));
    }

    Ok(())
}
