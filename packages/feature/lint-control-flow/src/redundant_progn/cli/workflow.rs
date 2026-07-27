use anyhow::Result;

use crate::redundant_progn::cli::args::RedundantPrognReportArgs;
use crate::redundant_progn::cli::render::print_redundant_progn_report;
use crate::redundant_progn::usecase::{
    RedundantPrognPolicyOptions, collect_redundant_progns, evaluate_redundant_progn_policy,
    summarize_redundant_progns,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn redundant_progn_report(args: RedundantPrognReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut progn_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_redundant_progns(file, dialect, &tree)?;
        progn_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_redundant_progns(progn_form_count, violations);
    let policy = evaluate_redundant_progn_policy(
        RedundantPrognPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_redundant_progn_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "redundant-progn-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
