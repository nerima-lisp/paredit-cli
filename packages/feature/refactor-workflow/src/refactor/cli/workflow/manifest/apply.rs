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
use super::undo::{resolve_journal_path, restore_from_journal};
use paredit_core_cli::CliResult;
use paredit_core_cli::safe_text;
use paredit_core_cli::shared::apply_byte_span_edits;
use paredit_core_cli::shared::stable_text_hash;
use paredit_core_cli::shared::{
    write_artifact_with_rollback, write_files_with_rollback_expected,
    write_files_with_rollback_expected_anchored,
};
use paredit_core_safety::external::VerificationCommand;
use paredit_core_safety::journal::{UndoJournal, UndoJournalFile};
use paredit_core_safety::scope::WriteScope;
use paredit_core_syntax::sexpr::SyntaxTree;
use std::time::Duration;

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

pub fn refactor_apply(args: RefactorApplyArgs) -> CliResult<()> {
    let loaded_manifest =
        read_refactor_manifest_file(&args.manifest, args.expect_manifest_hash.as_deref())?;
    let manifest = parse_refactor_apply_manifest(&loaded_manifest.value)?;
    let root_guard = args
        .root
        .as_deref()
        .map(RefactorRootGuard::new)
        .transpose()?;

    let verification = args
        .verify_command
        .as_deref()
        .map(|command| {
            VerificationCommand::new(command).map(|command| {
                command.with_budget(args.verify_timeout_ms.map(Duration::from_millis))
            })
        })
        .transpose()
        .map_err(|_| paredit_core_cli::ArgumentError::FlagCombination {
            message: "--verify-command is not runnable".to_owned(),
        })?;

    let mut files = Vec::with_capacity(manifest.files.len());
    let mut rewritten_outputs = Vec::with_capacity(manifest.files.len());
    // The pre-image of every file, kept for as long as the write might have to
    // be reversed. This is the whole reason an undo is possible at all: the
    // manifest records what each edit *became* and never what it replaced, and
    // this is the only moment both texts exist together.
    let mut journal_entries = Vec::with_capacity(manifest.files.len());
    let mut source_bytes = 0_u64;

    for file in &manifest.files {
        let (resolved_path, input, expected_original) =
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
        journal_entries.push(
            UndoJournalFile::record(resolved_path.clone(), &input, &edits).map_err(
                crate::error::RefactorContext::new(format!(
                    "cannot record an undo journal for {}",
                    file.path.display()
                )),
            )?,
        );
        let rewritten = apply_byte_span_edits(&input, edits)?;
        let output_hash = stable_text_hash(&rewritten);
        let output_parse_ok = SyntaxTree::parse_with_dialect(&rewritten, file.dialect).is_ok();
        let changed = rewritten != input;

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
            output_parse_ok,
            expected_output_parse_ok: file.output_parse_ok,
        });
        rewritten_outputs.push((resolved_path, rewritten, expected_original));
    }

    let stale_file_count = files
        .iter()
        .filter(|file| !file.input_hash_matches())
        .count();
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
                    .collect::<CliResult<Vec<_>>>()?;
                write_files_with_rollback_expected_anchored(anchored_outputs)?;
            }
            None => write_files_with_rollback_expected(written_outputs)?,
        }

        for index in &written_indexes {
            if let Some(file) = files.get_mut(*index) {
                file.written = true;
            }
        }

        // Only the files that were actually rewritten belong in the journal.
        // An entry for an unchanged file would be harmless but misleading: an
        // undo would report it as "nothing to do" beside files that really do
        // need putting back.
        let journal = UndoJournal::new(
            "refactor apply",
            written_indexes
                .iter()
                .filter_map(|index| journal_entries.get(*index).cloned())
                .collect(),
        );

        // The journal is written *before* the verification runs. If the
        // command fails and the rollback then fails too, the operator is left
        // with a tree that needs restoring by hand — and the file describing
        // how is the one thing that must already be on disk by then.
        if let Some(requested) = args.undo_out.as_deref() {
            let path = resolve_journal_path(requested, &journal.operation);
            write_artifact_with_rollback(
                path.clone(),
                format!("{}\n", serde_json::to_string_pretty(&journal.to_json())?),
            )
            .map_err(crate::error::RefactorContext::new(format!(
                "failed to write undo journal {}",
                path.display()
            )))?;
        }

        if let Some(verification) = &verification {
            run_verification_or_restore(verification, &journal, root_guard.as_ref())?;
        }
    }

    // Built from the *resolved* paths, not the manifest's spelling: a
    // disclosure that quotes `../src/core.lisp` back at the caller says
    // nothing about which file that is. Under `--root` the guard has already
    // canonicalized them; without one the manifest's own spelling is what
    // came back, so it is resolved here. Computed whether or not `--write`
    // was passed, so a dry run answers "what could this touch".
    let scoped_path =
        |path: &std::path::Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let write_scope = WriteScope::new(
        root_guard
            .as_ref()
            .map(|guard| guard.canonical_root.clone()),
        rewritten_outputs
            .iter()
            .zip(&files)
            .filter(|(_, file)| file.changed)
            .map(|((path, _, _), _)| scoped_path(path)),
        rewritten_outputs
            .iter()
            .zip(&files)
            .filter(|(_, file)| !file.changed)
            .map(|((path, _, _), _)| scoped_path(path)),
    );

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
        write_scope,
        files,
        summary,
    };

    print_refactor_apply_result(&result, args.output)?;

    if !can_apply {
        return Err(paredit_core_cli::error::FeatureRefusal::message(
    paredit_core_cli::diagnosis::ErrorCode::GateFailed,
    format!("refactor apply validation failed: manifest_policy_passed={}, manifest_outputs_parse={}, stale_files={}, output_hash_mismatches={}, parse_errors={}, manifest_flag_mismatches={}",
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

/// Runs the caller's check and puts every written file back if it fails.
///
/// The order matters and is not the obvious one: the restore happens *before*
/// the error is raised, so the process exits with the tree in the state it
/// started in. Reporting the failure first and restoring afterwards would be
/// identical when everything works and would leave a half-applied refactor
/// behind the moment the restore itself failed.
///
/// A restore that fails is reported as a separate, louder failure than the
/// verification that triggered it, because the two need different responses:
/// one means "the refactor was wrong", the other means "the working tree needs
/// attention right now".
fn run_verification_or_restore(
    verification: &VerificationCommand,
    journal: &UndoJournal,
    root_guard: Option<&RefactorRootGuard>,
) -> CliResult<()> {
    let outcome = verification
        .run()
        .map_err(crate::error::RefactorContext::new(
            "failed to run --verify-command after writing",
        ))?;
    if outcome.passed() {
        return Ok(());
    }

    report_verification_transcript(&outcome);

    match restore_from_journal(journal, root_guard) {
        Ok(restored) => Err(paredit_core_cli::error::FeatureRefusal::message(
            paredit_core_cli::diagnosis::ErrorCode::GateFailed,
            format!(
                "{}; restored {restored} file(s) to their pre-refactor content",
                outcome.failure_reason()
            ),
        )
        .into()),
        // Not a gate: the writes landed and undoing them did not, so the
        // working tree is in a state the caller has to look at. That is
        // `environment.rollback-failed`, and reporting it as a tripped gate
        // would tell an agent the operation simply did not happen.
        Err(error) => Err(paredit_core_cli::error::FeatureRefusal::message(
            paredit_core_cli::diagnosis::ErrorCode::EnvironmentRollbackFailed,
            format!(
                "{}; restoring the written files ALSO failed, so the working tree is \
                 partially refactored: {}",
                outcome.failure_reason(),
                paredit_core_cli::error::error_chain(&error),
            ),
        )
        .into()),
    }
}

/// Echoes the tail of a failed command's output to stderr.
///
/// Printed as its own lines rather than folded into the error message. The
/// error renderer escapes every control character in one value, which is right
/// for a message and turns a twenty-line test failure into one unreadable
/// string of `\u{a}`. Emitting the transcript line by line keeps the newlines
/// real while still escaping each line — the output comes from a process this
/// tool did not write, so ANSI and bidirectional overrides must not reach the
/// terminal untouched.
///
/// Bounded: a failing test suite can produce megabytes, and a message that
/// scrolls the failure off the top hides the line that matters.
fn report_verification_transcript(outcome: &paredit_core_safety::external::VerificationOutcome) {
    for (label, stream) in [("stdout", &outcome.stdout), ("stderr", &outcome.stderr)] {
        let trimmed = stream.trim_end();
        if trimmed.is_empty() {
            continue;
        }
        let lines = trimmed.lines().collect::<Vec<_>>();
        let skipped = lines.len().saturating_sub(VERIFY_TRANSCRIPT_LINES);
        if skipped > 0 {
            eprintln!(
                "--- verify-command {label} (last {VERIFY_TRANSCRIPT_LINES} of {} lines) ---",
                lines.len()
            );
        } else {
            eprintln!("--- verify-command {label} ---");
        }
        for line in lines.into_iter().skip(skipped) {
            eprintln!("{}", safe_text!(line));
        }
    }
}

/// How many trailing lines of each stream a verification failure quotes.
const VERIFY_TRANSCRIPT_LINES: usize = 20;

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use paredit_core_cli::args::OutputFormat;
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;
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
            undo_out: None,
            verify_command: None,
            verify_timeout_ms: None,
            output: OutputFormat::Json,
        })
        .expect_err("root replacement must be rejected");

        // `chain()`, not `{error:#}`: the alternate form was `anyhow`'s, and
        // a typed error's `Display` stops at the outermost message — here
        // "retained parent no longer matches", with the refusal underneath it.
        // The two sibling assertions on this refusal already read the chain.
        assert!(
            error.chain().contains("refusing replaced parent directory"),
            "unexpected error: {}",
            error.chain()
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
