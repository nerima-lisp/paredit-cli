use anyhow::Result;

use crate::application::usecase::negated_step_delta_report::{
    NegatedStepDeltaPolicyOptions, collect_negated_step_deltas, evaluate_negated_step_delta_policy,
    summarize_negated_step_deltas,
};
use crate::presentation::cli::negated_step_delta_report::args::NegatedStepDeltaReportArgs;
use crate::presentation::cli::negated_step_delta_report::render::print_negated_step_delta_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn negated_step_delta_report(
    args: NegatedStepDeltaReportArgs,
) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut step_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_negated_step_deltas(file, dialect, &tree)?;
        step_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_negated_step_deltas(step_form_count, violations);
    let policy = evaluate_negated_step_delta_policy(
        NegatedStepDeltaPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_negated_step_delta_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "negated-step-delta-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
