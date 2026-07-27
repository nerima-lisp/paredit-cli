use anyhow::Result;

use crate::application::usecase::empty_let_report::{
    EmptyLetPolicyOptions, collect_empty_lets, evaluate_empty_let_policy, summarize_empty_lets,
};
use crate::presentation::cli::empty_let_report::args::EmptyLetReportArgs;
use crate::presentation::cli::empty_let_report::render::print_empty_let_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn empty_let_report(args: EmptyLetReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut let_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_empty_lets(file, dialect, &tree)?;
        let_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_empty_lets(let_form_count, violations);
    let policy =
        evaluate_empty_let_policy(EmptyLetPolicyOptions::new(args.fail_on_violation), &summary);
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_empty_let_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "empty-let-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
