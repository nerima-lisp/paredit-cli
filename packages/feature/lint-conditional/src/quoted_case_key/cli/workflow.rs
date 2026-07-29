use paredit_core_cli::CommandResult;

use crate::quoted_case_key::cli::args::QuotedCaseKeyReportArgs;
use crate::quoted_case_key::cli::render::print_quoted_case_key_report;
use crate::quoted_case_key::usecase::{
    build_quoted_case_key_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn quoted_case_key_report(args: QuotedCaseKeyReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_quoted_case_key_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_quoted_case_key_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "quoted-case-key-report policy failed: {message}"
        )));
    }

    Ok(())
}
