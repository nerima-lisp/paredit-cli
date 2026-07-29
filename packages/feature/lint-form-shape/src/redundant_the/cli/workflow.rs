use paredit_core_cli::CommandResult;

use crate::redundant_the::cli::args::RedundantTheReportArgs;
use crate::redundant_the::cli::render::print_redundant_the_report;
use crate::redundant_the::usecase::{
    RedundantThePolicyOptions, collect_redundant_thes, evaluate_redundant_the_policy,
    summarize_redundant_thes,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn redundant_the_report(args: RedundantTheReportArgs) -> CommandResult {
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
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "redundant-the-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
