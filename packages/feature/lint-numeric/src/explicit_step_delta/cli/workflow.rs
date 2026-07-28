use anyhow::Result;

use crate::explicit_step_delta::cli::args::ExplicitStepDeltaReportArgs;
use crate::explicit_step_delta::cli::render::print_explicit_step_delta_report;
use crate::explicit_step_delta::usecase::{
    ExplicitStepDeltaPolicyOptions, collect_explicit_step_deltas,
    evaluate_explicit_step_delta_policy, summarize_explicit_step_deltas,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn explicit_step_delta_report(args: ExplicitStepDeltaReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut step_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) =
            collect_explicit_step_deltas(file, dialect, &tree)?;
        step_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_explicit_step_deltas(step_form_count, violations);
    let policy = evaluate_explicit_step_delta_policy(
        ExplicitStepDeltaPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_explicit_step_delta_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "explicit-step-delta-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
