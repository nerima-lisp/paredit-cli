use anyhow::Result;

use crate::last_default_count::cli::args::LastDefaultCountReportArgs;
use crate::last_default_count::cli::render::print_last_default_count_report;
use crate::last_default_count::usecase::{
    LastDefaultCountPolicyOptions, collect_last_default_counts, evaluate_last_default_count_policy,
    summarize_last_default_counts,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn last_default_count_report(args: LastDefaultCountReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut call_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_last_default_counts(file, dialect, &tree)?;
        call_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_last_default_counts(call_form_count, violations);
    let policy = evaluate_last_default_count_policy(
        LastDefaultCountPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_last_default_count_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "last-default-count-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
