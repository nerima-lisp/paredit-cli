use super::super::types::check::{
    RefactorCheckFileResult, RefactorCheckResult, RefactorCheckSummary,
};
use super::super::types::manifest::RefactorApplyManifestHeader;
use super::super::types::root::{RefactorRootGuard, RefactorRootReport};
use super::io::read_refactor_manifest_file;
use super::parse::parse_refactor_apply_manifest;
use super::root::{MAX_MANIFEST_SOURCE_TOTAL_BYTES, read_refactor_manifest_source};
use super::validation::validate_manifest_edits;
use anyhow::{Context, Result};
use paredit_core_cli::shared::apply_byte_span_edits;
use paredit_core_cli::shared::stable_text_hash;
use paredit_core_syntax::sexpr::SyntaxTree;
use std::path::Path as FsPath;

pub fn build_refactor_check_result(
    manifest_path: &FsPath,
    root: Option<&FsPath>,
    expected_manifest_hash: Option<&str>,
) -> Result<RefactorCheckResult> {
    let loaded_manifest = read_refactor_manifest_file(manifest_path, expected_manifest_hash)?;
    let manifest = parse_refactor_apply_manifest(&loaded_manifest.value)?;
    let root_guard = root.map(RefactorRootGuard::new).transpose()?;

    let manifest_policy_passed = manifest.policy_passed;
    let manifest_outputs_parse = manifest.all_outputs_parse;
    let mut files = Vec::with_capacity(manifest.files.len());
    let mut source_bytes = 0_u64;

    for file in &manifest.files {
        let (_resolved_path, input, _expected_original) =
            read_refactor_manifest_source(&file.path, root_guard.as_ref())?;
        source_bytes = source_bytes.saturating_add(input.len() as u64);
        if source_bytes > MAX_MANIFEST_SOURCE_TOTAL_BYTES {
            anyhow::bail!(
                "refusing manifest sources: cumulative input exceeds {MAX_MANIFEST_SOURCE_TOTAL_BYTES} bytes"
            );
        }
        let input_hash = stable_text_hash(&input);
        let input_hash_matches = input_hash == file.input_hash;
        let edits = file
            .edits
            .iter()
            .map(|edit| (edit.span, edit.replacement.clone()))
            .collect::<Vec<_>>();
        validate_manifest_edits(&input, &edits)
            .with_context(|| format!("manifest edits are invalid for {}", file.path.display()))?;
        let rewritten = apply_byte_span_edits(&input, edits)?;
        let output_hash = stable_text_hash(&rewritten);
        let output_hash_matches = output_hash == file.output_hash;
        let output_parse_ok = SyntaxTree::parse_with_dialect(&rewritten, file.dialect).is_ok();
        let changed = rewritten != input;
        let manifest_flags_match =
            changed == file.changed && output_parse_ok == file.output_parse_ok;
        let stale = !input_hash_matches;

        files.push(RefactorCheckFileResult {
            path: file.path.clone(),
            changed,
            expected_changed: file.changed,
            edit_count: file.edits.len(),
            input_hash,
            output_hash,
            expected_input_hash: file.input_hash.clone(),
            expected_output_hash: file.output_hash.clone(),
            input_hash_matches,
            output_hash_matches,
            output_parse_ok,
            expected_output_parse_ok: file.output_parse_ok,
            manifest_flags_match,
            stale,
        });
    }

    let stale_file_count = files.iter().filter(|file| file.stale).count();
    let output_hash_mismatch_count = files
        .iter()
        .filter(|file| !file.output_hash_matches)
        .count();
    let parse_error_count = files.iter().filter(|file| !file.output_parse_ok).count();
    let manifest_flag_mismatch_count = files
        .iter()
        .filter(|file| !file.manifest_flags_match)
        .count();
    let can_apply = manifest_policy_passed
        && manifest_outputs_parse
        && stale_file_count == 0
        && output_hash_mismatch_count == 0
        && parse_error_count == 0
        && manifest_flag_mismatch_count == 0;
    let changed_files = files
        .iter()
        .filter(|file| file.changed)
        .map(|file| file.path.display().to_string())
        .collect::<Vec<_>>();

    Ok(RefactorCheckResult {
        manifest: RefactorApplyManifestHeader {
            path: manifest_path.to_path_buf(),
            hash: loaded_manifest.hash,
            mode: manifest.mode,
            from: manifest.from,
            to: manifest.to,
        },
        root: RefactorRootReport::from_guard(root_guard.as_ref()),
        manifest_policy_passed,
        manifest_outputs_parse,
        summary: RefactorCheckSummary {
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
    })
}
