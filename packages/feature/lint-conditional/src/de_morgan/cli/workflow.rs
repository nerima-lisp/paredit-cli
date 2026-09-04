use paredit_core_cli::CommandResult;

use crate::de_morgan::cli::args::DeMorganReportArgs;
use crate::de_morgan::cli::render::print_de_morgan_report;
use crate::de_morgan::usecase::{build_de_morgan_report, evaluate_fail_on_violation_policy};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn de_morgan_report(args: DeMorganReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_de_morgan_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_de_morgan_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "de-morgan-report policy failed: {message}"
        )));
    }

    Ok(())
}
