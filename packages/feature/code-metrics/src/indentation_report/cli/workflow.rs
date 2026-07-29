use anyhow::Result;

use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::indentation_report::cli::args::IndentationReportArgs;
use crate::indentation_report::cli::render::print_deviation_report;
use crate::indentation_report::usecase::{
    build_indentation_report, evaluate_fail_on_deviation_policy,
};

pub fn indentation_report(args: IndentationReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_indentation_report(file, dialect, &tree));
    }

    let policy = evaluate_fail_on_deviation_policy(args.fail_on_deviation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_deviation_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect indentation policy failed: {message}"
        )));
    }

    Ok(())
}
