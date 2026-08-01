use paredit_core_cli::CommandResult;

use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::readtable_case_report::cli::args::ReadtableCaseReportArgs;
use crate::readtable_case_report::cli::render::print_fragile_report;
use crate::readtable_case_report::usecase::{
    build_readtable_case_report, evaluate_fail_on_fragile_policy,
};

pub fn readtable_case_report(args: ReadtableCaseReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_readtable_case_report(file, dialect, &tree));
    }

    let policy = evaluate_fail_on_fragile_policy(args.fail_on_fragile, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_fragile_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect readtable-case policy failed: {message}"
        )));
    }

    Ok(())
}
