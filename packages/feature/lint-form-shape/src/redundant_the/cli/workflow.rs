use anyhow::Result;

use crate::application::usecase::redundant_the_report::{
    RedundantThePolicyOptions, collect_redundant_thes, evaluate_redundant_the_policy,
    summarize_redundant_thes,
};
use crate::presentation::cli::redundant_the_report::args::RedundantTheReportArgs;
use crate::presentation::cli::redundant_the_report::render::print_redundant_the_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn redundant_the_report(
    args: RedundantTheReportArgs,
) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut the_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_redundant_thes(file, dialect, &tree)?;
        the_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_redundant_thes(the_form_count, violations);
    let policy = evaluate_redundant_the_policy(
        RedundantThePolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_redundant_the_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "redundant-the-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
