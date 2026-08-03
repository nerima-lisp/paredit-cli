use paredit_core_cli::CommandResult;

use crate::defconstant_non_eql_value::cli::args::DefconstantNonEqlValueReportArgs;
use crate::defconstant_non_eql_value::cli::render::print_defconstant_non_eql_value_report;
use crate::defconstant_non_eql_value::usecase::{
    build_defconstant_non_eql_value_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn defconstant_non_eql_value_report(args: DefconstantNonEqlValueReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_defconstant_non_eql_value_report(
            file, dialect, &tree,
        )?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_defconstant_non_eql_value_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "defconstant-non-eql-value-report policy failed: {message}"
        )));
    }

    Ok(())
}
