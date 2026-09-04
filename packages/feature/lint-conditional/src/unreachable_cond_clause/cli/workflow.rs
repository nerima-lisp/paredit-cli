use paredit_core_cli::CommandResult;

use crate::unreachable_cond_clause::cli::args::UnreachableCondClauseReportArgs;
use crate::unreachable_cond_clause::cli::render::print_unreachable_cond_clause_report;
use crate::unreachable_cond_clause::usecase::{
    build_unreachable_cond_clause_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn unreachable_cond_clause_report(args: UnreachableCondClauseReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_unreachable_cond_clause_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_unreachable_cond_clause_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "unreachable-cond-clause-report policy failed: {message}"
        )));
    }

    Ok(())
}
