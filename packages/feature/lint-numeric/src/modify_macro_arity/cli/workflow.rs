use paredit_core_cli::CommandResult;

use crate::modify_macro_arity::cli::args::ModifyMacroArityReportArgs;
use crate::modify_macro_arity::cli::render::print_modify_macro_arity_report;
use crate::modify_macro_arity::usecase::{
    build_modify_macro_arity_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn modify_macro_arity_report(args: ModifyMacroArityReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_modify_macro_arity_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_modify_macro_arity_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "modify-macro-arity-report policy failed: {message}"
        )));
    }

    Ok(())
}
