use paredit_core_cli::CommandResult;

use crate::append_list_to_cons::cli::args::AppendListToConsReportArgs;
use crate::append_list_to_cons::cli::render::print_append_list_to_cons_report;
use crate::append_list_to_cons::usecase::{
    collect_append_list_to_cons, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn append_list_to_cons_report(args: AppendListToConsReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(collect_append_list_to_cons(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_append_list_to_cons_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "append-list-to-cons-report policy failed: {message}"
        )));
    }

    Ok(())
}
