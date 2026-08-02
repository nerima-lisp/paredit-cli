use paredit_core_cli::CommandResult;

use crate::block_name_shadows_outer_block::cli::args::BlockNameShadowsOuterBlockReportArgs;
use crate::block_name_shadows_outer_block::cli::render::print_block_name_shadows_outer_block_report;
use crate::block_name_shadows_outer_block::usecase::{
    build_block_name_shadows_outer_block_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn block_name_shadows_outer_block_report(
    args: BlockNameShadowsOuterBlockReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_block_name_shadows_outer_block_report(
            file, dialect, &tree,
        )?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_block_name_shadows_outer_block_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "block-name-shadows-outer-block-report policy failed: {message}"
        )));
    }

    Ok(())
}
