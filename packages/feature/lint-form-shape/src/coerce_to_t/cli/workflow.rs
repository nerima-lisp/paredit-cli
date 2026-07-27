use anyhow::Result;

use crate::application::usecase::coerce_to_t_report::{
    CoerceToTPolicyOptions, collect_coerce_to_ts, evaluate_coerce_to_t_policy,
    summarize_coerce_to_ts,
};
use crate::presentation::cli::coerce_to_t_report::args::CoerceToTReportArgs;
use crate::presentation::cli::coerce_to_t_report::render::print_coerce_to_t_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn coerce_to_t_report(args: CoerceToTReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut coerce_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_coerce_to_ts(file, dialect, &tree)?;
        coerce_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_coerce_to_ts(coerce_form_count, violations);
    let policy = evaluate_coerce_to_t_policy(
        CoerceToTPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_coerce_to_t_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "coerce-to-t-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
