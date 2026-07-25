use anyhow::Result;

use crate::application::usecase::manual_push_report::{
    ManualPushPolicyOptions, collect_manual_pushes, evaluate_manual_push_policy,
    summarize_manual_pushes,
};
use crate::presentation::cli::manual_push_report::args::ManualPushReportArgs;
use crate::presentation::cli::manual_push_report::render::print_manual_push_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn manual_push_report(args: ManualPushReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut assignment_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_manual_pushes(file, dialect, &tree)?;
        assignment_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_manual_pushes(assignment_form_count, violations);
    let policy = evaluate_manual_push_policy(
        ManualPushPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_manual_push_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "manual-push-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
