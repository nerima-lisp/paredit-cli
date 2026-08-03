use paredit_core_cli::CommandResult;

use crate::apply_with_literal_collection::cli::args::ApplyWithLiteralCollectionReportArgs;
use crate::apply_with_literal_collection::cli::render::print_apply_with_literal_collection_report;
use crate::apply_with_literal_collection::usecase::{
    build_apply_with_literal_collection_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn apply_with_literal_collection_report(
    args: ApplyWithLiteralCollectionReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_apply_with_literal_collection_report(
            file, dialect, &tree,
        )?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_apply_with_literal_collection_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "apply-with-literal-collection-report policy failed: {message}"
        )));
    }

    Ok(())
}
