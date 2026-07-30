use paredit_core_cli::CommandResult;

use crate::single_clause_cond::cli::args::SingleClauseCondReportArgs;
use crate::single_clause_cond::cli::render::print_single_clause_cond_report;
use crate::single_clause_cond::usecase::{
    build_single_clause_cond_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn single_clause_cond_report(args: SingleClauseCondReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_single_clause_cond_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_single_clause_cond_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "single-clause-cond-report policy failed: {message}"
        )));
    }

    Ok(())
}
