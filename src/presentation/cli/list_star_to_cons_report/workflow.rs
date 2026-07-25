use anyhow::Result;

use crate::application::usecase::list_star_to_cons_report::{
    ListStarToConsPolicyOptions, collect_list_star_to_cons, evaluate_list_star_to_cons_policy,
    summarize_list_star_to_cons,
};
use crate::presentation::cli::list_star_to_cons_report::args::ListStarToConsReportArgs;
use crate::presentation::cli::list_star_to_cons_report::render::print_list_star_to_cons_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn list_star_to_cons_report(
    args: ListStarToConsReportArgs,
) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut list_star_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_list_star_to_cons(file, dialect, &tree)?;
        list_star_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_list_star_to_cons(list_star_form_count, violations);
    let policy = evaluate_list_star_to_cons_policy(
        ListStarToConsPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_list_star_to_cons_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "list-star-to-cons-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
