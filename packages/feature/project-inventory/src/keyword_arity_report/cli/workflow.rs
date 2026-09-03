use paredit_core_cli::CommandResult;

use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::keyword_arity_report::cli::args::KeywordArityReportArgs;
use crate::keyword_arity_report::cli::render::print_fault_report;
use crate::keyword_arity_report::usecase::{
    build_keyword_arity_report, evaluate_fail_on_fault_policy,
};

pub fn keyword_arity_report(args: KeywordArityReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_keyword_arity_report(file, dialect, &tree));
    }

    let policy = evaluate_fail_on_fault_policy(args.fail_on_fault, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_fault_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect keyword-arity policy failed: {message}"
        )));
    }

    Ok(())
}
