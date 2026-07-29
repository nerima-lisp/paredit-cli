use paredit_core_cli::CommandResult;

use crate::cond_t_clause::cli::args::CondTClauseReportArgs;
use crate::cond_t_clause::cli::render::print_cond_t_clause_report;
use crate::cond_t_clause::usecase::{
    CondTClausePolicyOptions, collect_cond_t_clauses, evaluate_cond_t_clause_policy,
    summarize_cond_t_clauses,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn cond_t_clause_report(args: CondTClauseReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut cond_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_cond_t_clauses(file, dialect, &tree)?;
        cond_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_cond_t_clauses(cond_form_count, violations);
    let policy = evaluate_cond_t_clause_policy(
        CondTClausePolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_cond_t_clause_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "cond-t-clause-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
