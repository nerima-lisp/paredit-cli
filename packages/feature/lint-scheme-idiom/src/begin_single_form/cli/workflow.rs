use paredit_core_cli::CommandResult;

use crate::begin_single_form::cli::args::BeginSingleFormReportArgs;
use crate::begin_single_form::cli::render::print_begin_single_form_report;
use crate::begin_single_form::usecase::{
    build_begin_single_form_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn begin_single_form_report(args: BeginSingleFormReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_begin_single_form_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_begin_single_form_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "scheme-begin-single-form-report policy failed: {message}"
        )));
    }

    Ok(())
}
