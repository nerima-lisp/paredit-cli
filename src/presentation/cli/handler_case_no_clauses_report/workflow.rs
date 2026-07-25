use anyhow::Result;

use crate::application::usecase::handler_case_no_clauses_report::{
    HandlerCaseNoClausesPolicyOptions, collect_handler_case_no_clauses,
    evaluate_handler_case_no_clauses_policy, summarize_handler_case_no_clauses,
};
use crate::presentation::cli::handler_case_no_clauses_report::args::HandlerCaseNoClausesReportArgs;
use crate::presentation::cli::handler_case_no_clauses_report::render::print_handler_case_no_clauses_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn handler_case_no_clauses_report(
    args: HandlerCaseNoClausesReportArgs,
) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut handler_case_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) =
            collect_handler_case_no_clauses(file, dialect, &tree)?;
        handler_case_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_handler_case_no_clauses(handler_case_form_count, violations);
    let policy = evaluate_handler_case_no_clauses_policy(
        HandlerCaseNoClausesPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_handler_case_no_clauses_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "handler-case-no-clauses-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
