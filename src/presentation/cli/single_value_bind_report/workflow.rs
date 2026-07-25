use anyhow::Result;

use crate::application::usecase::single_value_bind_report::{
    SingleValueBindPolicyOptions, collect_single_value_binds, evaluate_single_value_bind_policy,
    summarize_single_value_binds,
};
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};
use crate::presentation::cli::single_value_bind_report::args::SingleValueBindReportArgs;
use crate::presentation::cli::single_value_bind_report::render::print_single_value_bind_report;

pub(in crate::presentation::cli) fn single_value_bind_report(
    args: SingleValueBindReportArgs,
) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut bind_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_single_value_binds(file, dialect, &tree)?;
        bind_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_single_value_binds(bind_form_count, violations);
    let policy = evaluate_single_value_bind_policy(
        SingleValueBindPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_single_value_bind_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "single-value-bind-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
