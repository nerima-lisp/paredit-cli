use paredit_core_cli::CommandResult;

use crate::flet_single_use_inlinable::cli::args::FletSingleUseInlinableReportArgs;
use crate::flet_single_use_inlinable::cli::render::print_flet_single_use_inlinable_report;
use crate::flet_single_use_inlinable::usecase::{
    build_flet_single_use_inlinable_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn flet_single_use_inlinable_report(args: FletSingleUseInlinableReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_flet_single_use_inlinable_report(
            file, dialect, &tree,
        )?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_flet_single_use_inlinable_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "flet-single-use-inlinable-report policy failed: {message}"
        )));
    }

    Ok(())
}
