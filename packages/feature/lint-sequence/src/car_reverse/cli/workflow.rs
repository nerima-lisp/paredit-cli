use paredit_core_cli::CommandResult;

use crate::car_reverse::cli::args::CarReverseReportArgs;
use crate::car_reverse::cli::render::print_car_reverse_report;
use crate::car_reverse::usecase::{collect_car_reverses, evaluate_fail_on_violation_policy};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn car_reverse_report(args: CarReverseReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(collect_car_reverses(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_car_reverse_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "car-reverse-report policy failed: {message}"
        )));
    }

    Ok(())
}
