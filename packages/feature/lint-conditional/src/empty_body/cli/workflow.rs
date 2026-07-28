use anyhow::Result;

use crate::empty_body::cli::args::EmptyBodyReportArgs;
use crate::empty_body::cli::render::print_empty_body_report;
use crate::empty_body::usecase::{
    EmptyBodyPolicyOptions, collect_empty_bodies, evaluate_empty_body_policy,
    summarize_empty_bodies,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn empty_body_report(args: EmptyBodyReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut body_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_empty_bodies(file, dialect, &tree)?;
        body_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_empty_bodies(body_form_count, violations);
    let policy = evaluate_empty_body_policy(
        EmptyBodyPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_empty_body_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "empty-body-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
