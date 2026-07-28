use anyhow::Result;

use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::read_conditional_report::cli::args::ReadConditionalReportArgs;
use crate::read_conditional_report::cli::render::print_conditional_report;
use crate::read_conditional_report::usecase::{
    build_read_conditional_report, evaluate_fail_on_conditional_policy,
};

pub fn read_conditional_report(args: ReadConditionalReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_read_conditional_report(file, dialect, &tree));
    }

    let policy = evaluate_fail_on_conditional_policy(args.fail_on_conditional, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_conditional_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect read-conditionals policy failed: {message}"
        )));
    }

    Ok(())
}
