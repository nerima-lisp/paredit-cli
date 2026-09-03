use paredit_core_cli::CommandResult;

use crate::hash_table_iteration_order_assumed::cli::args::HashTableIterationOrderAssumedReportArgs;
use crate::hash_table_iteration_order_assumed::cli::render::print_hash_table_iteration_order_assumed_report;
use crate::hash_table_iteration_order_assumed::usecase::{
    collect_hash_order_assumptions, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn hash_table_iteration_order_assumed_report(
    args: HashTableIterationOrderAssumedReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(collect_hash_order_assumptions(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_hash_table_iteration_order_assumed_report(
        &reports,
        &policy,
        args.output,
        args.verbosity,
    )?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "hash-table-iteration-order-assumed-report policy failed: {message}"
        )));
    }

    Ok(())
}
