use paredit_core_cli::CommandResult;

use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::symbol_function_fset_dynamic_name::cli::args::SymbolFunctionFsetDynamicNameReportArgs;
use crate::symbol_function_fset_dynamic_name::cli::render::print_symbol_function_fset_dynamic_name_report;
use crate::symbol_function_fset_dynamic_name::usecase::{
    build_symbol_function_fset_dynamic_name_report, evaluate_fail_on_violation_policy,
};

pub fn symbol_function_fset_dynamic_name_report(
    args: SymbolFunctionFsetDynamicNameReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_symbol_function_fset_dynamic_name_report(
            file, dialect, &tree,
        )?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_symbol_function_fset_dynamic_name_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "symbol-function-fset-dynamic-name-report policy failed: {message}"
        )));
    }

    Ok(())
}
