use super::super::super::args::RefactorDiffArgs;
use super::super::super::manifest::io::read_refactor_manifest_file;
use super::super::super::manifest::parse::parse_refactor_apply_manifest;
use super::super::super::manifest::root::{
    MAX_MANIFEST_SOURCE_TOTAL_BYTES, read_refactor_manifest_source,
};
use super::super::super::manifest::validation::validate_manifest_edits;
use super::super::super::render::print_refactor_diff_result;
use super::super::super::types::diff::{
    RefactorDiffFileResult, RefactorDiffResult, RefactorDiffSummary,
};
use super::super::super::types::manifest::RefactorApplyManifestHeader;
use super::super::super::types::root::{RefactorRootGuard, RefactorRootReport};
use paredit_core_cli::CliResult;
use paredit_core_cli::shared::apply_byte_span_edits;
use paredit_core_cli::shared::stable_text_hash;
use paredit_core_cli::shared::unified_diff;
use paredit_core_syntax::sexpr::SyntaxTree;

pub fn refactor_diff(args: RefactorDiffArgs) -> CliResult<()> {
    let loaded_manifest =
        read_refactor_manifest_file(&args.manifest, args.expect_manifest_hash.as_deref())?;
    let manifest = parse_refactor_apply_manifest(&loaded_manifest.value)?;
    let root_guard = args
        .root
        .as_deref()
        .map(RefactorRootGuard::new)
        .transpose()?;

    let mut files = Vec::with_capacity(manifest.files.len());
    let mut source_bytes = 0_u64;

    for file in &manifest.files {
        let (_resolved_path, input, _expected_original) =
            read_refactor_manifest_source(&file.path, root_guard.as_ref())?;
        source_bytes = source_bytes.saturating_add(input.len() as u64);
        if source_bytes > MAX_MANIFEST_SOURCE_TOTAL_BYTES {
            return Err(paredit_core_cli::error::FeatureRefusal::message(
    paredit_core_cli::diagnosis::ErrorCode::RefusalInputTooLarge,
    format!("refusing manifest sources: cumulative input exceeds {MAX_MANIFEST_SOURCE_TOTAL_BYTES} bytes"),
)
.into());
        }
        let input_hash = stable_text_hash(&input);
        let edits = file
            .edits
            .iter()
            .map(|edit| (edit.span, edit.replacement.clone()))
            .collect::<Vec<_>>();
        validate_manifest_edits(&input, &edits).map_err(crate::error::RefactorContext::new(
            format!("manifest edits are invalid for {}", file.path.display()),
        ))?;
        let rewritten = apply_byte_span_edits(&input, edits)?;
        let output_hash = stable_text_hash(&rewritten);
        let output_parse_ok = SyntaxTree::parse_with_dialect(&rewritten, file.dialect).is_ok();
        let changed = rewritten != input;
        let diff = if changed {
            unified_diff(&file.path, &input, &rewritten)
        } else {
            String::new()
        };

        files.push(RefactorDiffFileResult {
            path: file.path.clone(),
            changed,
            expected_changed: file.changed,
            edit_count: file.edits.len(),
            input_hash,
            output_hash,
            expected_input_hash: file.input_hash.clone(),
            expected_output_hash: file.output_hash.clone(),
            output_parse_ok,
            expected_output_parse_ok: file.output_parse_ok,
            diff,
        });
    }

    let stale_file_count = files.iter().filter(|file| file.stale()).count();
    let output_hash_mismatch_count = files
        .iter()
        .filter(|file| !file.output_hash_matches())
        .count();
    let parse_error_count = files.iter().filter(|file| !file.output_parse_ok).count();
    let manifest_flag_mismatch_count = files
        .iter()
        .filter(|file| !file.manifest_flags_match())
        .count();
    let can_apply = manifest.policy_passed
        && manifest.all_outputs_parse
        && stale_file_count == 0
        && output_hash_mismatch_count == 0
        && parse_error_count == 0
        && manifest_flag_mismatch_count == 0;
    let changed_files = files
        .iter()
        .filter(|file| file.changed)
        .map(|file| file.path.display().to_string())
        .collect::<Vec<_>>();

    let result = RefactorDiffResult {
        manifest: RefactorApplyManifestHeader {
            path: args.manifest,
            hash: loaded_manifest.hash,
            mode: manifest.mode,
            from: manifest.from,
            to: manifest.to,
        },
        root: RefactorRootReport::from_guard(root_guard.as_ref()),
        manifest_policy_passed: manifest.policy_passed,
        manifest_outputs_parse: manifest.all_outputs_parse,
        summary: RefactorDiffSummary {
            file_count: files.len(),
            changed_file_count: changed_files.len(),
            changed_files,
            edit_count: files.iter().map(|file| file.edit_count).sum(),
            stale_file_count,
            output_hash_mismatch_count,
            parse_error_count,
            manifest_flag_mismatch_count,
            can_apply,
        },
        files,
    };

    print_refactor_diff_result(&result, args.output)?;

    if !can_apply {
        return Err(paredit_core_cli::error::FeatureRefusal::message(
    paredit_core_cli::diagnosis::ErrorCode::GateFailed,
    format!("refactor diff validation failed: manifest_policy_passed={}, manifest_outputs_parse={}, stale_files={}, output_hash_mismatches={}, parse_errors={}, manifest_flag_mismatches={}",
            result.manifest_policy_passed,
            result.manifest_outputs_parse,
            result.summary.stale_file_count,
            result.summary.output_hash_mismatch_count,
            result.summary.parse_error_count,
            result.summary.manifest_flag_mismatch_count),
)
.into());
    }

    Ok(())
}
