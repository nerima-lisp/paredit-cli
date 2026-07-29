use paredit_core_cli::CommandResult;

use crate::getf_default_nil::cli::args::GetfDefaultNilReportArgs;
use crate::getf_default_nil::cli::render::print_getf_default_nil_report;
use crate::getf_default_nil::usecase::{
    build_getf_default_nil_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn getf_default_nil_report(args: GetfDefaultNilReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_getf_default_nil_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_getf_default_nil_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "getf-default-nil-report policy failed: {message}"
        )));
    }

    Ok(())
}
