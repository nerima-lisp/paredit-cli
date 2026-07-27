use anyhow::Result;

use crate::malformed_cond_clause::cli::args::MalformedCondClauseReportArgs;
use crate::malformed_cond_clause::cli::render::print_malformed_cond_clause_report;
use crate::malformed_cond_clause::usecase::{
    MalformedCondClausePolicyOptions, collect_malformed_cond_clauses,
    evaluate_malformed_cond_clause_policy, summarize_malformed_cond_clauses,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn malformed_cond_clause_report(args: MalformedCondClauseReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut cond_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_cond_form_count, file_violations) =
            collect_malformed_cond_clauses(file, dialect, &tree)?;
        cond_form_count += file_cond_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_malformed_cond_clauses(cond_form_count, violations);
    let policy = evaluate_malformed_cond_clause_policy(
        MalformedCondClausePolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_malformed_cond_clause_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "malformed-cond-clause-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
