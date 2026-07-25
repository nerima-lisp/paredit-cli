use anyhow::Result;

use crate::application::usecase::nthcdr_zero_report::{
    NthcdrZeroPolicyOptions, collect_nthcdr_zeros, evaluate_nthcdr_zero_policy,
    summarize_nthcdr_zeros,
};
use crate::presentation::cli::nthcdr_zero_report::args::NthcdrZeroReportArgs;
use crate::presentation::cli::nthcdr_zero_report::render::print_nthcdr_zero_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn nthcdr_zero_report(args: NthcdrZeroReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut nthcdr_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_nthcdr_zeros(file, dialect, &tree)?;
        nthcdr_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_nthcdr_zeros(nthcdr_form_count, violations);
    let policy = evaluate_nthcdr_zero_policy(
        NthcdrZeroPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_nthcdr_zero_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "nthcdr-zero-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
