use anyhow::Result;

use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::read_time_eval_report::cli::args::ReadTimeEvalReportArgs;
use crate::read_time_eval_report::cli::render::print_read_eval_report;
use crate::read_time_eval_report::usecase::{
    build_read_time_eval_report, evaluate_fail_on_read_eval_policy,
};

pub fn read_time_eval_report(args: ReadTimeEvalReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_read_time_eval_report(file, dialect, &tree));
    }

    let policy = evaluate_fail_on_read_eval_policy(args.fail_on_read_eval, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_read_eval_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect read-time-eval policy failed: {message}"
        )));
    }

    Ok(())
}
