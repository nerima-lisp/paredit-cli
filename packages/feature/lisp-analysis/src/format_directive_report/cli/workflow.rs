use anyhow::Result;

use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::format_directive_report::cli::args::FormatDirectiveReportArgs;
use crate::format_directive_report::cli::render::print_mismatch_report;
use crate::format_directive_report::usecase::{
    build_format_directive_report, evaluate_fail_on_mismatch_policy,
};

pub fn format_directive_report(args: FormatDirectiveReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_format_directive_report(file, dialect, &tree));
    }

    let policy = evaluate_fail_on_mismatch_policy(args.fail_on_mismatch, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_mismatch_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect format-directives policy failed: {message}"
        )));
    }

    Ok(())
}
