use paredit_core_cli::CommandResult;

use crate::manual_pushnew::cli::args::ManualPushnewReportArgs;
use crate::manual_pushnew::cli::render::print_manual_pushnew_report;
use crate::manual_pushnew::usecase::{
    build_manual_pushnew_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn manual_pushnew_report(args: ManualPushnewReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_manual_pushnew_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_manual_pushnew_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "manual-pushnew-report policy failed: {message}"
        )));
    }

    Ok(())
}
