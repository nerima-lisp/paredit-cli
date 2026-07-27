use anyhow::Result;

use crate::application::usecase::car_reverse_report::{
    CarReversePolicyOptions, collect_car_reverses, evaluate_car_reverse_policy,
    summarize_car_reverses,
};
use crate::presentation::cli::car_reverse_report::args::CarReverseReportArgs;
use crate::presentation::cli::car_reverse_report::render::print_car_reverse_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn car_reverse_report(args: CarReverseReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut accessor_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_car_reverses(file, dialect, &tree)?;
        accessor_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_car_reverses(accessor_form_count, violations);
    let policy = evaluate_car_reverse_policy(
        CarReversePolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_car_reverse_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "car-reverse-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
