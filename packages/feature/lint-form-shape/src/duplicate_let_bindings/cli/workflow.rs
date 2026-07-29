use paredit_core_cli::CommandResult;

use crate::duplicate_let_bindings::cli::args::DuplicateLetBindingReportArgs;
use crate::duplicate_let_bindings::cli::render::print_duplicate_let_binding_report;
use crate::duplicate_let_bindings::usecase::{
    build_duplicate_let_binding_report, evaluate_fail_on_duplicate_policy,
};
use paredit_core_cli::shared::read_input_dialect_and_tree;

pub fn duplicate_let_binding_report(args: DuplicateLetBindingReportArgs) -> CommandResult {
    let mut reports = Vec::with_capacity(args.files.len());
    for file in &args.files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_duplicate_let_binding_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_duplicate_policy(args.fail_on_duplicate, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_duplicate_let_binding_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "duplicate-let-binding-report policy failed: {message}"
        )));
    }

    Ok(())
}
