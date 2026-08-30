use paredit_core_cli::{CliResult, CommandResult};

use crate::block_name_shadows_outer_block::cli::args::BlockNameShadowsOuterBlockReportArgs;
use crate::block_name_shadows_outer_block::cli::render::print_block_name_shadows_outer_block_report;
use crate::block_name_shadows_outer_block::usecase::{
    build_block_name_shadows_outer_block_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{
    analyze_files_raw, expand_input_files, note_partial_file_failures, read_input_dialect_and_tree,
    total_file_failure,
};

pub fn block_name_shadows_outer_block_report(
    args: BlockNameShadowsOuterBlockReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let analysis = analyze_files_raw(&files, |file| {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        CliResult::Ok(build_block_name_shadows_outer_block_report(
            file, dialect, &tree,
        )?)
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

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
