use paredit_core_cli::CommandResult;

use crate::cond_t_clause::cli::args::CondTClauseReportArgs;
use crate::cond_t_clause::cli::render::print_cond_t_clause_report;
use crate::cond_t_clause::usecase::{
    build_cond_t_clause_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn cond_t_clause_report(args: CondTClauseReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_cond_t_clause_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_cond_t_clause_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "cond-t-clause-report policy failed: {message}"
        )));
    }

    Ok(())
}
