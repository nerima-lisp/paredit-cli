use paredit_core_cli::CommandResult;

use crate::manual_push::cli::args::ManualPushReportArgs;
use crate::manual_push::cli::render::print_manual_push_report;
use crate::manual_push::usecase::{build_manual_push_report, evaluate_fail_on_violation_policy};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn manual_push_report(args: ManualPushReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_manual_push_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_manual_push_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "manual-push-report policy failed: {message}"
        )));
    }

    Ok(())
}
