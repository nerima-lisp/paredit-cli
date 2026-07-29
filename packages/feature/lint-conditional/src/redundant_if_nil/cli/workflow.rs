use paredit_core_cli::CommandResult;

use crate::redundant_if_nil::cli::args::RedundantIfNilReportArgs;
use crate::redundant_if_nil::cli::render::print_redundant_if_nil_report;
use crate::redundant_if_nil::usecase::{
    RedundantIfNilPolicyOptions, collect_redundant_if_nils, evaluate_redundant_if_nil_policy,
    summarize_redundant_if_nils,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn redundant_if_nil_report(args: RedundantIfNilReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut if_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_redundant_if_nils(file, dialect, &tree)?;
        if_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_redundant_if_nils(if_form_count, violations);
    let policy = evaluate_redundant_if_nil_policy(
        RedundantIfNilPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_redundant_if_nil_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "redundant-if-nil-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
