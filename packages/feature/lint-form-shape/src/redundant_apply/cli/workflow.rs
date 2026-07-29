use paredit_core_cli::CommandResult;

use crate::redundant_apply::cli::args::RedundantApplyReportArgs;
use crate::redundant_apply::cli::render::print_redundant_apply_report;
use crate::redundant_apply::usecase::{
    RedundantApplyPolicyOptions, collect_redundant_applies, evaluate_redundant_apply_policy,
    summarize_redundant_applies,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn redundant_apply_report(args: RedundantApplyReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut apply_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_redundant_applies(file, dialect, &tree)?;
        apply_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_redundant_applies(apply_form_count, violations);
    let policy = evaluate_redundant_apply_policy(
        RedundantApplyPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_redundant_apply_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "redundant-apply-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
