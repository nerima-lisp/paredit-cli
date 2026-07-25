use anyhow::Result;

use crate::application::usecase::subseq_zero_report::{
    SubseqZeroPolicyOptions, collect_subseq_zeros, evaluate_subseq_zero_policy,
    summarize_subseq_zeros,
};
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};
use crate::presentation::cli::subseq_zero_report::args::SubseqZeroReportArgs;
use crate::presentation::cli::subseq_zero_report::render::print_subseq_zero_report;

pub(in crate::presentation::cli) fn subseq_zero_report(args: SubseqZeroReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut subseq_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_subseq_zeros(file, dialect, &tree)?;
        subseq_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_subseq_zeros(subseq_form_count, violations);
    let policy = evaluate_subseq_zero_policy(
        SubseqZeroPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_subseq_zero_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "subseq-zero-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
