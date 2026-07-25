use anyhow::Result;

use crate::application::usecase::redundant_apply_report::{
    RedundantApplyPolicyOptions, collect_redundant_applies, evaluate_redundant_apply_policy,
    summarize_redundant_applies,
};
use crate::presentation::cli::redundant_apply_report::args::RedundantApplyReportArgs;
use crate::presentation::cli::redundant_apply_report::render::print_redundant_apply_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn redundant_apply_report(
    args: RedundantApplyReportArgs,
) -> Result<()> {
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
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "redundant-apply-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
