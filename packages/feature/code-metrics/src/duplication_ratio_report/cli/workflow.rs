use paredit_core_cli::CommandResult;

use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::duplication_ratio_report::cli::args::DuplicationRatioReportArgs;
use crate::duplication_ratio_report::cli::render::print_duplication_report;
use crate::duplication_ratio_report::usecase::{
    build_duplication_ratio_report, evaluate_fail_on_duplication_policy,
};

pub fn duplication_ratio_report(args: DuplicationRatioReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_duplication_ratio_report(file, dialect, &tree));
    }

    let policy = evaluate_fail_on_duplication_policy(args.fail_on_duplication, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_duplication_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect duplication-ratio policy failed: {message}"
        )));
    }

    Ok(())
}
