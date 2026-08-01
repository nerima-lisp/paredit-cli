use paredit_core_cli::CommandResult;

use crate::car_nthcdr::cli::args::CarNthcdrReportArgs;
use crate::car_nthcdr::cli::render::print_car_nthcdr_report;
use crate::car_nthcdr::usecase::{collect_car_nthcdrs, evaluate_fail_on_violation_policy};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn car_nthcdr_report(args: CarNthcdrReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(collect_car_nthcdrs(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_car_nthcdr_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "car-nthcdr-report policy failed: {message}"
        )));
    }

    Ok(())
}
