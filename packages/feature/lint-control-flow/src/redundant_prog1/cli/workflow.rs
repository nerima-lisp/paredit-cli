use paredit_core_cli::CommandResult;

use crate::redundant_prog1::cli::args::RedundantProg1ReportArgs;
use crate::redundant_prog1::cli::render::print_redundant_prog1_report;
use crate::redundant_prog1::usecase::{
    build_redundant_prog1_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn redundant_prog1_report(args: RedundantProg1ReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_redundant_prog1_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_redundant_prog1_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "redundant-prog1-report policy failed: {message}"
        )));
    }

    Ok(())
}
