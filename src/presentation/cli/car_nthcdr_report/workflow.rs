use anyhow::Result;

use crate::application::usecase::car_nthcdr_report::{
    CarNthcdrPolicyOptions, collect_car_nthcdrs, evaluate_car_nthcdr_policy, summarize_car_nthcdrs,
};
use crate::presentation::cli::car_nthcdr_report::args::CarNthcdrReportArgs;
use crate::presentation::cli::car_nthcdr_report::render::print_car_nthcdr_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn car_nthcdr_report(args: CarNthcdrReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut car_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_car_nthcdrs(file, dialect, &tree)?;
        car_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_car_nthcdrs(car_form_count, violations);
    let policy = evaluate_car_nthcdr_policy(
        CarNthcdrPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_car_nthcdr_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "car-nthcdr-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
