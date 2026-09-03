use paredit_core_cli::CommandResult;

use crate::handler_case_no_clauses::cli::args::HandlerCaseNoClausesReportArgs;
use crate::handler_case_no_clauses::cli::render::print_handler_case_no_clauses_report;
use crate::handler_case_no_clauses::usecase::{
    build_handler_case_no_clauses_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn handler_case_no_clauses_report(args: HandlerCaseNoClausesReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_handler_case_no_clauses_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_handler_case_no_clauses_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "handler-case-no-clauses-report policy failed: {message}"
        )));
    }

    Ok(())
}
