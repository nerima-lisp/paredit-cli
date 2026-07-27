use anyhow::Result;

use crate::malformed_case_clause::cli::args::MalformedCaseClauseReportArgs;
use crate::malformed_case_clause::cli::render::print_malformed_case_clause_report;
use crate::malformed_case_clause::usecase::{
    MalformedCaseClausePolicyOptions, collect_malformed_case_clauses,
    evaluate_malformed_case_clause_policy, summarize_malformed_case_clauses,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn malformed_case_clause_report(args: MalformedCaseClauseReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut case_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_case_form_count, file_violations) =
            collect_malformed_case_clauses(file, dialect, &tree)?;
        case_form_count += file_case_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_malformed_case_clauses(case_form_count, violations);
    let policy = evaluate_malformed_case_clause_policy(
        MalformedCaseClausePolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_malformed_case_clause_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "malformed-case-clause-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
