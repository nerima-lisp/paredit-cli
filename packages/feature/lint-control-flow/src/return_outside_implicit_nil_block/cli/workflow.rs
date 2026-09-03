use paredit_core_cli::CommandResult;

use crate::return_outside_implicit_nil_block::cli::args::ReturnOutsideImplicitNilBlockReportArgs;
use crate::return_outside_implicit_nil_block::cli::render::print_return_outside_implicit_nil_block_report;
use crate::return_outside_implicit_nil_block::usecase::{
    build_return_outside_implicit_nil_block_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn return_outside_implicit_nil_block_report(
    args: ReturnOutsideImplicitNilBlockReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_return_outside_implicit_nil_block_report(
            file, dialect, &tree,
        )?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_return_outside_implicit_nil_block_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "return-outside-implicit-nil-block-report policy failed: {message}"
        )));
    }

    Ok(())
}
