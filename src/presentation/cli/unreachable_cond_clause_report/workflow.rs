use anyhow::Result;

use crate::application::usecase::unreachable_cond_clause_report::{
    UnreachableCondClausePolicyOptions, collect_unreachable_cond_clauses,
    evaluate_unreachable_cond_clause_policy, summarize_unreachable_cond_clauses,
};
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};
use crate::presentation::cli::unreachable_cond_clause_report::args::UnreachableCondClauseReportArgs;
use crate::presentation::cli::unreachable_cond_clause_report::render::print_unreachable_cond_clause_report;

pub(in crate::presentation::cli) fn unreachable_cond_clause_report(
    args: UnreachableCondClauseReportArgs,
) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut cond_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_cond_form_count, file_violations) =
            collect_unreachable_cond_clauses(file, dialect, &tree)?;
        cond_form_count += file_cond_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_unreachable_cond_clauses(cond_form_count, violations);
    let policy = evaluate_unreachable_cond_clause_policy(
        UnreachableCondClausePolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_unreachable_cond_clause_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "unreachable-cond-clause-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
