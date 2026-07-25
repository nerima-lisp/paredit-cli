use anyhow::Result;

use crate::application::usecase::single_clause_cond_report::{
    SingleClauseCondPolicyOptions, collect_single_clause_conds, evaluate_single_clause_cond_policy,
    summarize_single_clause_conds,
};
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};
use crate::presentation::cli::single_clause_cond_report::args::SingleClauseCondReportArgs;
use crate::presentation::cli::single_clause_cond_report::render::print_single_clause_cond_report;

pub(in crate::presentation::cli) fn single_clause_cond_report(
    args: SingleClauseCondReportArgs,
) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut cond_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_single_clause_conds(file, dialect, &tree)?;
        cond_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_single_clause_conds(cond_form_count, violations);
    let policy = evaluate_single_clause_cond_policy(
        SingleClauseCondPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_single_clause_cond_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "single-clause-cond-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
