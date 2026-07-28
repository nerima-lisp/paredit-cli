use anyhow::Result;

use crate::redundant_body_progn::cli::args::RedundantBodyPrognReportArgs;
use crate::redundant_body_progn::cli::render::print_redundant_body_progn_report;
use crate::redundant_body_progn::usecase::{
    RedundantBodyPrognPolicyOptions, collect_redundant_body_progns,
    evaluate_redundant_body_progn_policy, summarize_redundant_body_progns,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn redundant_body_progn_report(args: RedundantBodyPrognReportArgs) -> Result<()> {
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
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "redundant-body-progn-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
