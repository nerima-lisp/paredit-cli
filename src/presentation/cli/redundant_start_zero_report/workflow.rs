use anyhow::Result;

use crate::application::usecase::redundant_start_zero_report::{
    RedundantStartZeroPolicyOptions, collect_redundant_start_zeros,
    evaluate_redundant_start_zero_policy, summarize_redundant_start_zeros,
};
use crate::presentation::cli::redundant_start_zero_report::args::RedundantStartZeroReportArgs;
use crate::presentation::cli::redundant_start_zero_report::render::print_redundant_start_zero_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn redundant_start_zero_report(
    args: RedundantStartZeroReportArgs,
) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut call_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) =
            collect_redundant_start_zeros(file, dialect, &tree)?;
        call_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_redundant_start_zeros(call_form_count, violations);
    let policy = evaluate_redundant_start_zero_policy(
        RedundantStartZeroPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_redundant_start_zero_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "redundant-start-zero-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
