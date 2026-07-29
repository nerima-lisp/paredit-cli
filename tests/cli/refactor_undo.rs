//! `refactor apply --undo-out` and `refactor undo`, plus the `--verify-command`
//! rollback that shares their machinery.
//!
//! The property under test throughout is that a write is *reversible on its own
//! terms*: not "git can restore it", which is true of anything, but that the
//! tool itself can put back exactly what it replaced and can tell when it no
//! longer may.

use super::*;

use serde_json::Value;
use std::path::{Path, PathBuf};

const ORIGINAL: &str = "(defun old-name (x) (* x 2))\n\n(defun caller () (old-name 3))\n";
const RENAMED: &str = "(defun new-name (x) (* x 2))\n\n(defun caller () (new-name 3))\n";

/// Sets up a directory holding one source file and its rename preview manifest.
fn workspace_with_manifest(label: &str, source_text: &str, from: &str, to: &str) -> PathBuf {
    let dir = fresh_temp_dir(label);
    fs::write(dir.join("core.lisp"), source_text).expect("write source");

    let manifest = paredit()
        .current_dir(&dir)
        .args(["refactor", "preview", "--mode", "symbol"])
        .args(["--from", from, "--to", to])
        .args(["core.lisp", "--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    fs::write(dir.join("preview.json"), manifest).expect("write manifest");
    dir
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).expect("read file")
}

#[test]
fn apply_records_a_journal_of_reverse_edits() {
    let dir = workspace_with_manifest("undo-record", ORIGINAL, "old-name", "new-name");

    paredit()
        .current_dir(&dir)
        .args(["refactor", "apply", "--manifest", "preview.json", "--write"])
        .args(["--undo-out", "undo.json"])
        .assert()
        .success();

    assert_eq!(read(&dir.join("core.lisp")), RENAMED);

    let journal: Value =
        serde_json::from_str(&read(&dir.join("undo.json"))).expect("journal is JSON");
    assert_eq!(journal["schema_version"], 1);
    assert_eq!(journal["operation"], "refactor apply");
    assert_eq!(journal["file_count"], 1);

    // The journal's replacements are the *old* text: that is the whole point,
    // since the manifest records only what each edit became.
    let edits = journal["files"][0]["edits"].as_array().expect("edits");
    assert_eq!(edits.len(), 2);
    for edit in edits {
        assert_eq!(edit["replacement"], "old-name");
    }
}

/// A journal is only written when asked for. It records the content of every
/// file that changed, so writing one unrequested would be a surprising
/// disclosure as well as a surprising file.
#[test]
fn no_journal_is_written_without_the_flag() {
    let dir = workspace_with_manifest("undo-optional", ORIGINAL, "old-name", "new-name");

    paredit()
        .current_dir(&dir)
        .args(["refactor", "apply", "--manifest", "preview.json", "--write"])
        .assert()
        .success();

    assert!(!dir.join("undo.json").exists());
    assert_eq!(read(&dir.join("core.lisp")), RENAMED);
}

/// `--undo-out` without `--write` would record a reversal of a write that never
/// happened.
#[test]
fn a_journal_cannot_be_requested_for_a_dry_run() {
    let dir = workspace_with_manifest("undo-dry", ORIGINAL, "old-name", "new-name");

    paredit()
        .current_dir(&dir)
        .args(["refactor", "apply", "--manifest", "preview.json"])
        .args(["--undo-out", "undo.json"])
        .assert()
        .failure();
}

#[test]
fn undo_restores_the_recorded_pre_image() {
    let dir = workspace_with_manifest("undo-restore", ORIGINAL, "old-name", "new-name");
    paredit()
        .current_dir(&dir)
        .args(["refactor", "apply", "--manifest", "preview.json", "--write"])
        .args(["--undo-out", "undo.json"])
        .assert()
        .success();

    paredit()
        .current_dir(&dir)
        .args(["refactor", "undo", "--journal", "undo.json", "--write"])
        .assert()
        .success();

    assert_eq!(read(&dir.join("core.lisp")), ORIGINAL);
}

#[test]
fn undo_without_write_reports_and_changes_nothing() {
    let dir = workspace_with_manifest("undo-report", ORIGINAL, "old-name", "new-name");
    paredit()
        .current_dir(&dir)
        .args(["refactor", "apply", "--manifest", "preview.json", "--write"])
        .args(["--undo-out", "undo.json"])
        .assert()
        .success();

    let output = paredit()
        .current_dir(&dir)
        .args(["refactor", "undo", "--journal", "undo.json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: Value = serde_json::from_slice(&output).expect("undo report is JSON");

    assert_eq!(report["status"], "restorable");
    assert_eq!(report["summary"]["restorable_file_count"], 1);
    assert_eq!(report["summary"]["restored_file_count"], 0);
    assert_eq!(report["summary"]["applied"], false);
    assert_eq!(read(&dir.join("core.lisp")), RENAMED);
}

/// After a successful undo the file holds the pre-image, whose hash is not the
/// post-image the journal requires. Replaying must therefore refuse, rather
/// than "undoing" a change that is no longer there.
#[test]
fn a_journal_cannot_be_applied_twice() {
    let dir = workspace_with_manifest("undo-twice", ORIGINAL, "old-name", "new-name");
    paredit()
        .current_dir(&dir)
        .args(["refactor", "apply", "--manifest", "preview.json", "--write"])
        .args(["--undo-out", "undo.json"])
        .assert()
        .success();
    paredit()
        .current_dir(&dir)
        .args(["refactor", "undo", "--journal", "undo.json", "--write"])
        .assert()
        .success();

    paredit()
        .current_dir(&dir)
        .args(["refactor", "undo", "--journal", "undo.json", "--write"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("refactor undo refused"));

    assert_eq!(read(&dir.join("core.lisp")), ORIGINAL);
}

/// The guard that matters most in practice: somebody kept working after the
/// refactor. Undoing would silently discard their edit, so it must not.
#[test]
fn undo_refuses_a_file_edited_since_the_write() {
    let dir = workspace_with_manifest("undo-drift", ORIGINAL, "old-name", "new-name");
    paredit()
        .current_dir(&dir)
        .args(["refactor", "apply", "--manifest", "preview.json", "--write"])
        .args(["--undo-out", "undo.json"])
        .assert()
        .success();

    let edited = format!("{RENAMED};; a later edit\n");
    fs::write(dir.join("core.lisp"), &edited).expect("simulate a later edit");

    paredit()
        .current_dir(&dir)
        .args(["refactor", "undo", "--journal", "undo.json", "--write"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("refactor undo refused"));

    assert_eq!(read(&dir.join("core.lisp")), edited);
}

#[test]
fn a_journal_from_an_unsupported_schema_is_refused() {
    let dir = fresh_temp_dir("undo-schema");
    fs::write(
        dir.join("undo.json"),
        r#"{"schema_version": 99, "operation": "x", "tool_version": "0", "files": []}"#,
    )
    .expect("write journal");

    paredit()
        .current_dir(&dir)
        .args(["refactor", "undo", "--journal", "undo.json"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("schema version 99"));
}

#[test]
fn a_journal_that_is_not_json_names_the_file() {
    let dir = fresh_temp_dir("undo-garbage");
    fs::write(dir.join("undo.json"), "not json at all").expect("write journal");

    paredit()
        .current_dir(&dir)
        .args(["refactor", "undo", "--journal", "undo.json"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("is not JSON"));
}

// --- --verify-command ---

#[test]
fn a_passing_verification_leaves_the_write_in_place() {
    let dir = workspace_with_manifest("verify-pass", ORIGINAL, "old-name", "new-name");

    paredit()
        .current_dir(&dir)
        .args(["refactor", "apply", "--manifest", "preview.json", "--write"])
        .args(["--verify-command", "exit 0"])
        .assert()
        .success();

    assert_eq!(read(&dir.join("core.lisp")), RENAMED);
}

/// The point of the feature: the project's own checks, not the tool's opinion,
/// decide whether the refactor stands.
#[test]
fn a_failing_verification_restores_every_written_file() {
    let dir = workspace_with_manifest("verify-fail", ORIGINAL, "old-name", "new-name");

    paredit()
        .current_dir(&dir)
        .args(["refactor", "apply", "--manifest", "preview.json", "--write"])
        .args(["--verify-command", "exit 1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "restored 1 file(s) to their pre-refactor content",
        ));

    assert_eq!(read(&dir.join("core.lisp")), ORIGINAL);
}

/// A failure the operator cannot diagnose is a failure they will disable. The
/// command's own output has to reach them.
#[test]
fn a_failing_verification_echoes_the_commands_output() {
    let dir = workspace_with_manifest("verify-transcript", ORIGINAL, "old-name", "new-name");

    paredit()
        .current_dir(&dir)
        .args(["refactor", "apply", "--manifest", "preview.json", "--write"])
        .args([
            "--verify-command",
            "echo 'ran 3 tests'; echo 'FAIL: bad arity' >&2; exit 2",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("ran 3 tests"))
        .stderr(predicate::str::contains("FAIL: bad arity"))
        .stderr(predicate::str::contains("exited with status 2"));
}

/// A verification that hangs must not hang the refactor. The budget kills it
/// and the write is rolled back, because a check that did not finish is not a
/// check that passed.
#[test]
fn a_verification_that_outlives_its_budget_rolls_back() {
    let dir = workspace_with_manifest("verify-timeout", ORIGINAL, "old-name", "new-name");

    paredit()
        .current_dir(&dir)
        .args(["refactor", "apply", "--manifest", "preview.json", "--write"])
        .args(["--verify-command", "sleep 30"])
        .args(["--verify-timeout-ms", "100"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("exceeded its budget"));

    assert_eq!(read(&dir.join("core.lisp")), ORIGINAL);
}

/// Verification only makes sense against files that were actually written.
#[test]
fn a_verification_cannot_be_requested_for_a_dry_run() {
    let dir = workspace_with_manifest("verify-dry", ORIGINAL, "old-name", "new-name");

    paredit()
        .current_dir(&dir)
        .args(["refactor", "apply", "--manifest", "preview.json"])
        .args(["--verify-command", "exit 1"])
        .assert()
        .failure();

    assert_eq!(read(&dir.join("core.lisp")), ORIGINAL);
}

/// The journal is written before the verification runs, so that a rollback
/// which itself fails still leaves the operator holding the instructions.
#[test]
fn the_journal_survives_a_failed_verification() {
    let dir = workspace_with_manifest("verify-journal", ORIGINAL, "old-name", "new-name");

    paredit()
        .current_dir(&dir)
        .args(["refactor", "apply", "--manifest", "preview.json", "--write"])
        .args(["--undo-out", "undo.json"])
        .args(["--verify-command", "exit 1"])
        .assert()
        .failure();

    assert!(
        dir.join("undo.json").is_file(),
        "the journal must exist even though the write was rolled back"
    );
    assert_eq!(read(&dir.join("core.lisp")), ORIGINAL);
}
