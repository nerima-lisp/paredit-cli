use anyhow::Result;

use crate::application::usecase::redundant_body_progn_report::{
    RedundantBodyPrognPolicyOptions, collect_redundant_body_progns,
    evaluate_redundant_body_progn_policy, summarize_redundant_body_progns,
};
use crate::presentation::cli::redundant_body_progn_report::args::RedundantBodyPrognReportArgs;
use crate::presentation::cli::redundant_body_progn_report::render::print_redundant_body_progn_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn redundant_body_progn_report(
    args: RedundantBodyPrognReportArgs,
) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut implicit_progn_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) =
            collect_redundant_body_progns(file, dialect, &tree)?;
        implicit_progn_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_redundant_body_progns(implicit_progn_form_count, violations);
    let policy = evaluate_redundant_body_progn_policy(
        RedundantBodyPrognPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_redundant_body_progn_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "redundant-body-progn-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
