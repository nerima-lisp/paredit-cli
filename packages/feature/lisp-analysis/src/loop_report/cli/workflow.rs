use anyhow::Result;

use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::loop_report::cli::args::LoopReportArgs;
use crate::loop_report::cli::render::print_unterminated_report;
use crate::loop_report::usecase::{build_loop_report, evaluate_fail_on_unterminated_policy};

pub fn loop_report(args: LoopReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_loop_report(file, dialect, &tree));
    }

    let policy = evaluate_fail_on_unterminated_policy(args.fail_on_unterminated, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_unterminated_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect loop policy failed: {message}"
        )));
    }

    Ok(())
}
