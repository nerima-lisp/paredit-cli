use anyhow::Result;

use crate::application::usecase::double_reverse_report::{
    DoubleReversePolicyOptions, collect_double_reverses, evaluate_double_reverse_policy,
    summarize_double_reverses,
};
use crate::presentation::cli::double_reverse_report::args::DoubleReverseReportArgs;
use crate::presentation::cli::double_reverse_report::render::print_double_reverse_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn double_reverse_report(
    args: DoubleReverseReportArgs,
) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reverse_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_double_reverses(file, dialect, &tree)?;
        reverse_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_double_reverses(reverse_form_count, violations);
    let policy = evaluate_double_reverse_policy(
        DoubleReversePolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_double_reverse_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "double-reverse-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
