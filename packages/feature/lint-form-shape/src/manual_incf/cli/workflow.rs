use anyhow::Result;

use crate::application::usecase::manual_incf_report::{
    ManualIncfPolicyOptions, collect_manual_incfs, evaluate_manual_incf_policy,
    summarize_manual_incfs,
};
use crate::presentation::cli::manual_incf_report::args::ManualIncfReportArgs;
use crate::presentation::cli::manual_incf_report::render::print_manual_incf_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn manual_incf_report(args: ManualIncfReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut assignment_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_manual_incfs(file, dialect, &tree)?;
        assignment_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_manual_incfs(assignment_form_count, violations);
    let policy = evaluate_manual_incf_policy(
        ManualIncfPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_manual_incf_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "manual-incf-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
