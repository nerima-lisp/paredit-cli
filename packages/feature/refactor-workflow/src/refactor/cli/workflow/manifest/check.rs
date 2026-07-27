use super::super::super::super::*;
use super::super::super::args::RefactorCheckArgs;
use super::super::super::manifest::check::build_refactor_check_result;
use super::super::super::render::print_refactor_check_result;

pub(in crate::presentation::cli) fn refactor_check(args: RefactorCheckArgs) -> Result<()> {
    let result = build_refactor_check_result(
        &args.manifest,
        args.root.as_deref(),
        args.expect_manifest_hash.as_deref(),
    )?;

    print_refactor_check_result(&result, args.output)?;

    if !result.summary.can_apply {
        anyhow::bail!(
            "refactor check validation failed: manifest_policy_passed={}, manifest_outputs_parse={}, stale_files={}, output_hash_mismatches={}, parse_errors={}, manifest_flag_mismatches={}",
            result.manifest_policy_passed,
            result.manifest_outputs_parse,
            result.summary.stale_file_count,
            result.summary.output_hash_mismatch_count,
            result.summary.parse_error_count,
            result.summary.manifest_flag_mismatch_count
        );
    }

    Ok(())
}
