use anyhow::Result;

use crate::application::usecase::redundant_let_star_report::{
    RedundantLetStarPolicyOptions, collect_redundant_let_stars, evaluate_redundant_let_star_policy,
    summarize_redundant_let_stars,
};
use crate::presentation::cli::redundant_let_star_report::args::RedundantLetStarReportArgs;
use crate::presentation::cli::redundant_let_star_report::render::print_redundant_let_star_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn redundant_let_star_report(
    args: RedundantLetStarReportArgs,
) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut let_star_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_redundant_let_stars(file, dialect, &tree)?;
        let_star_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_redundant_let_stars(let_star_form_count, violations);
    let policy = evaluate_redundant_let_star_policy(
        RedundantLetStarPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_redundant_let_star_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "redundant-let-star-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
