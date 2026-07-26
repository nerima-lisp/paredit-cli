use super::super::super::super::*;
use super::super::super::args::RefactorApplyArgs;
use super::super::super::manifest::io::read_refactor_manifest_file;
use super::super::super::manifest::parse::parse_refactor_apply_manifest;
use super::super::super::manifest::root::{
    MAX_MANIFEST_SOURCE_TOTAL_BYTES, read_refactor_manifest_source,
};
use super::super::super::manifest::validation::validate_manifest_edits;
use super::super::super::render::print_refactor_apply_result;
use super::super::super::types::apply::{
    RefactorApplyFileResult, RefactorApplyResult, RefactorApplySummary,
};
use super::super::super::types::manifest::RefactorApplyManifestHeader;
use super::super::super::types::root::{RefactorRootGuard, RefactorRootReport};
use crate::presentation::cli::shared::{
    write_files_with_rollback_expected, write_files_with_rollback_expected_anchored,
};

#[cfg(all(test, unix))]
thread_local! {
    static BEFORE_MANIFEST_WRITE_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        std::cell::RefCell::new(None);
}

#[cfg(all(test, unix))]
fn install_before_manifest_write_hook(hook: impl FnOnce() + 'static) {
    BEFORE_MANIFEST_WRITE_HOOK.with(|slot| {
        let previous = slot.replace(Some(Box::new(hook)));
        assert!(previous.is_none(), "manifest write hook already installed");
    });
}

#[cfg(all(test, unix))]
fn run_before_manifest_write_hook() {
    let hook = BEFORE_MANIFEST_WRITE_HOOK.with(|slot| slot.borrow_mut().take());
    if let Some(hook) = hook {
        hook();
    }
}

pub(in crate::presentation::cli) fn refactor_apply(args: RefactorApplyArgs) -> Result<()> {
    let loaded_manifest =
        read_refactor_manifest_file(&args.manifest, args.expect_manifest_hash.as_deref())?;
    let manifest = parse_refactor_apply_manifest(&loaded_manifest.value)?;
    let root_guard = args
        .root
        .as_deref()
        .map(RefactorRootGuard::new)
        .transpose()?;

    let mut files = Vec::with_capacity(manifest.files.len());
    let mut rewritten_outputs = Vec::with_capacity(manifest.files.len());
    let mut source_bytes = 0_u64;

    for file in &manifest.files {
        let (resolved_path, input, expected_original) =
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

        files.push(RefactorApplyFileResult {
            path: file.path.clone(),
            changed,
            expected_changed: file.changed,
            written: false,
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
        });
        rewritten_outputs.push((resolved_path, rewritten, expected_original));
    }

    let stale_file_count = files.iter().filter(|file| !file.input_hash_matches).count();
    let output_hash_mismatch_count = files
        .iter()
        .filter(|file| !file.output_hash_matches)
        .count();
    let parse_error_count = files.iter().filter(|file| !file.output_parse_ok).count();
    let manifest_flag_mismatch_count = files
        .iter()
        .filter(|file| !file.manifest_flags_match)
        .count();
    let can_apply = manifest.policy_passed
        && manifest.all_outputs_parse
        && stale_file_count == 0
        && output_hash_mismatch_count == 0
        && parse_error_count == 0
        && manifest_flag_mismatch_count == 0;

    if args.write && can_apply {
        let mut written_outputs = Vec::new();
        let mut written_indexes = Vec::new();
        for (index, (resolved_path, rewritten, expected_original)) in
            rewritten_outputs.iter().enumerate()
        {
            if files.get(index).is_some_and(|file| file.changed) {
                written_outputs.push((
                    resolved_path.clone(),
                    rewritten.clone(),
                    *expected_original,
                ));
                written_indexes.push(index);
            }
        }

        #[cfg(all(test, unix))]
        run_before_manifest_write_hook();

        match root_guard.as_ref() {
            Some(root_guard) => {
                let anchored_outputs = written_outputs
                    .into_iter()
                    .map(|(path, content, expected)| {
                        root_guard.anchored_manifest_write(path, content, expected)
                    })
                    .collect::<Result<Vec<_>>>()?;
                write_files_with_rollback_expected_anchored(anchored_outputs)?;
            }
            None => write_files_with_rollback_expected(written_outputs)?,
        }

        for index in written_indexes {
            if let Some(file) = files.get_mut(index) {
                file.written = true;
            }
        }
    }

    let changed_files = files
        .iter()
        .filter(|file| file.changed)
        .map(|file| file.path.display().to_string())
        .collect::<Vec<_>>();
    let summary = RefactorApplySummary {
        file_count: files.len(),
        changed_file_count: changed_files.len(),
        changed_files,
        written_file_count: files.iter().filter(|file| file.written).count(),
        edit_count: files.iter().map(|file| file.edit_count).sum(),
        stale_file_count,
        output_hash_mismatch_count,
        parse_error_count,
        manifest_flag_mismatch_count,
        applied: args.write && can_apply,
    };
    let result = RefactorApplyResult {
        manifest: RefactorApplyManifestHeader {
            path: args.manifest,
            hash: loaded_manifest.hash,
            mode: manifest.mode,
            from: manifest.from,
            to: manifest.to,
        },
        root: RefactorRootReport::from_guard(root_guard.as_ref()),
        write_requested: args.write,
        manifest_policy_passed: manifest.policy_passed,
        manifest_outputs_parse: manifest.all_outputs_parse,
        files,
        summary,
    };

    print_refactor_apply_result(&result, args.output)?;

    if !can_apply {
        anyhow::bail!(
            "refactor apply validation failed: manifest_policy_passed={}, manifest_outputs_parse={}, stale_files={}, output_hash_mismatches={}, parse_errors={}, manifest_flag_mismatches={}",
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

    fn fresh_temp_dir(label: &str) -> PathBuf {
        let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("paredit-{label}-{}-{id}", std::process::id()))
    }

    #[test]
    fn full_apply_rejects_same_inode_after_root_replacement() {
        let base = fresh_temp_dir("full-apply-root-swap");
        let root = base.join("workspace");
        let displaced_root = base.join("workspace-a");
        let source = root.join("core.lisp");
        let displaced_source = displaced_root.join("core.lisp");
        let manifest_path = base.join("preview.json");
        let original = "(old)\n";
        let rewritten = "(new)\n";

        fs::create_dir_all(&root).expect("create original root");
        fs::write(&source, original).expect("write original source");
        fs::write(
            &manifest_path,
            serde_json::to_string_pretty(&json!({
                "mode": "symbol",
                "from": "old",
                "to": "new",
                "summary": { "all_outputs_parse": true },
                "policy": { "passed": true },
                "files": [{
                    "path": source.display().to_string(),
                    "dialect": "common-lisp",
                    "changed": true,
                    "output_parse_ok": true,
                    "input_hash": stable_text_hash(original),
                    "output_hash": stable_text_hash(rewritten),
                    "edits": [{ "start": 1, "end": 4, "replacement": "new" }]
                }]
            }))
            .expect("serialize manifest"),
        )
        .expect("write manifest");

        let hook_root = root.clone();
        let hook_displaced_root = displaced_root.clone();
        let hook_source = source.clone();
        let hook_displaced_source = displaced_source.clone();
        install_before_manifest_write_hook(move || {
            fs::rename(&hook_root, &hook_displaced_root).expect("displace original root");
            fs::create_dir(&hook_root).expect("create replacement root");
            fs::hard_link(&hook_displaced_source, &hook_source)
                .expect("link retained source into replacement root");
            fs::remove_file(&hook_displaced_source)
                .expect("remove source from retained original root");
        });

        let error = refactor_apply(RefactorApplyArgs {
            manifest: manifest_path,
            expect_manifest_hash: None,
            root: Some(root.clone()),
            write: true,
            output: OutputFormat::Json,
        })
        .expect_err("root replacement must be rejected");

        assert!(
            format!("{error:#}").contains("refusing replaced parent directory"),
            "unexpected error: {error:#}"
        );
        assert_eq!(
            fs::read_to_string(&source).expect("read replacement-root source"),
            original
        );
        assert!(
            !displaced_source.exists(),
            "retained original root must not be rewritten through the replacement path"
        );

        fs::remove_dir_all(base).expect("remove test directory");
    }
}
