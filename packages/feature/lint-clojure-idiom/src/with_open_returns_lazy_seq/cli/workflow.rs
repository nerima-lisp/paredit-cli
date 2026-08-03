use paredit_core_cli::CommandResult;

use crate::with_open_returns_lazy_seq::cli::args::WithOpenLazySeqReportArgs;
use crate::with_open_returns_lazy_seq::cli::render::print_with_open_returns_lazy_seq_report;
use crate::with_open_returns_lazy_seq::usecase::{
    build_with_open_returns_lazy_seq_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn with_open_returns_lazy_seq_report(args: WithOpenLazySeqReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_with_open_returns_lazy_seq_report(
            file, dialect, &tree,
        )?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_with_open_returns_lazy_seq_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "with-open-returns-lazy-seq-report policy failed: {message}"
        )));
    }

    Ok(())
}
