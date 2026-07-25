use anyhow::Result;

use crate::application::usecase::cons_to_list_report::{
    ConsToListPolicyOptions, collect_cons_to_lists, evaluate_cons_to_list_policy,
    summarize_cons_to_lists,
};
use crate::presentation::cli::cons_to_list_report::args::ConsToListReportArgs;
use crate::presentation::cli::cons_to_list_report::render::print_cons_to_list_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn cons_to_list_report(args: ConsToListReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut cons_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_cons_to_lists(file, dialect, &tree)?;
        cons_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_cons_to_lists(cons_form_count, violations);
    let policy = evaluate_cons_to_list_policy(
        ConsToListPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_cons_to_list_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "cons-to-list-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
