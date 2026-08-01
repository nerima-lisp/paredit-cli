//! `refactor create-checkpoint` / `list-checkpoints` / `restore-checkpoint` /
//! `delete-checkpoint`: a named point in time an agent can come back to
//! across separate CLI invocations, built on the same undo-journal content
//! hashing `refactor undo` uses.
//!
//! The property under test throughout is the one FR-008 exists for: a
//! checkpoint that no longer matches what is on disk must refuse to restore,
//! whether the drift came from this tool or from a person's editor, because
//! the two are indistinguishable from a content hash alone.

use super::*;

use serde_json::Value;

const ORIGINAL: &str = "(defun greet (name) (format t \"hi ~a\" name))\n";

fn workspace_with_source(label: &str) -> PathBuf {
    let dir = fresh_temp_dir(label);
    fs::write(dir.join("core.lisp"), ORIGINAL).expect("write source");
    dir
}

fn read(dir: &std::path::Path, name: &str) -> String {
    fs::read_to_string(dir.join(name)).expect("read file")
}

#[test]
fn create_checkpoint_records_the_current_content() {
    let dir = workspace_with_source("checkpoint-create");

    let output = paredit()
        .current_dir(&dir)
        .args(["refactor", "create-checkpoint", "--name", "before-rename"])
        .arg("core.lisp")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: Value = serde_json::from_slice(&output).expect("create report is JSON");

    assert_eq!(report["name"], "before-rename");
    assert_eq!(report["replaced_existing"], false);
    assert_eq!(report["files"].as_array().expect("files").len(), 1);

    let stored = fs::read_to_string(dir.join(".paredit/checkpoints/before-rename.json"))
        .expect("checkpoint file exists");
    let stored: Value = serde_json::from_str(&stored).expect("checkpoint is JSON");
    assert_eq!(stored["name"], "before-rename");
    assert_eq!(stored["journal"]["file_count"], 1);
}

#[test]
fn a_duplicate_checkpoint_name_is_refused_without_force() {
    let dir = workspace_with_source("checkpoint-duplicate");
    paredit()
        .current_dir(&dir)
        .args([
            "refactor",
            "create-checkpoint",
            "--name",
            "cp1",
            "core.lisp",
        ])
        .assert()
        .success();

    paredit()
        .current_dir(&dir)
        .args([
            "refactor",
            "create-checkpoint",
            "--name",
            "cp1",
            "core.lisp",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));

    // --force replaces it and succeeds.
    paredit()
        .current_dir(&dir)
        .args([
            "refactor",
            "create-checkpoint",
            "--name",
            "cp1",
            "--force",
            "core.lisp",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"replaced_existing\": true"));
}

#[test]
fn an_invalid_checkpoint_name_is_refused() {
    let dir = workspace_with_source("checkpoint-invalid-name");
    paredit()
        .current_dir(&dir)
        .args([
            "refactor",
            "create-checkpoint",
            "--name",
            "../escape",
            "core.lisp",
        ])
        .assert()
        .failure();
}

#[test]
fn list_checkpoints_reports_name_and_files() {
    let dir = workspace_with_source("checkpoint-list");
    paredit()
        .current_dir(&dir)
        .args([
            "refactor",
            "create-checkpoint",
            "--name",
            "cp1",
            "core.lisp",
        ])
        .assert()
        .success();

    let output = paredit()
        .current_dir(&dir)
        .args(["refactor", "list-checkpoints"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: Value = serde_json::from_slice(&output).expect("list report is JSON");

    assert_eq!(report["checkpoint_count"], 1);
    assert_eq!(report["checkpoints"][0]["name"], "cp1");
    assert_eq!(
        report["checkpoints"][0]["files"]
            .as_array()
            .expect("files")
            .len(),
        1
    );
}

#[test]
fn list_checkpoints_is_empty_when_none_were_created() {
    let dir = fresh_temp_dir("checkpoint-list-empty");
    let output = paredit()
        .current_dir(&dir)
        .args(["refactor", "list-checkpoints"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: Value = serde_json::from_slice(&output).expect("list report is JSON");
    assert_eq!(report["checkpoint_count"], 0);
}

/// The positive path: nothing touched the file after the checkpoint, so
/// restoring it is a confirmed no-op that reproduces the recorded content.
#[test]
fn restore_succeeds_when_the_file_is_unchanged_since_the_checkpoint() {
    let dir = workspace_with_source("checkpoint-restore-unchanged");
    paredit()
        .current_dir(&dir)
        .args([
            "refactor",
            "create-checkpoint",
            "--name",
            "cp1",
            "core.lisp",
        ])
        .assert()
        .success();

    paredit()
        .current_dir(&dir)
        .args(["refactor", "restore-checkpoint", "--name", "cp1", "--write"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\": \"restored\""));

    assert_eq!(read(&dir, "core.lisp"), ORIGINAL);
}

/// Without `--write`, restore only reports — nothing changes on disk, even
/// after the covered file has drifted from the checkpoint.
#[test]
fn restore_without_write_reports_and_changes_nothing() {
    let dir = workspace_with_source("checkpoint-restore-report-only");
    paredit()
        .current_dir(&dir)
        .args([
            "refactor",
            "create-checkpoint",
            "--name",
            "cp1",
            "core.lisp",
        ])
        .assert()
        .success();

    let modified = format!("{ORIGINAL};; edited after the checkpoint\n");
    fs::write(dir.join("core.lisp"), &modified).expect("modify file");

    let output = paredit()
        .current_dir(&dir)
        .args(["refactor", "restore-checkpoint", "--name", "cp1"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: Value = serde_json::from_slice(&output).expect("restore report is JSON");

    assert_eq!(report["status"], "blocked");
    assert_eq!(report["summary"]["restored_file_count"], 0);
    assert_eq!(report["write_requested"], false);
    assert_eq!(read(&dir, "core.lisp"), modified);
}

/// The critical safety property: a file changed after the checkpoint was
/// taken — simulated here exactly as a hand edit would look, a plain
/// filesystem write the checkpoint has no way to know about — must refuse to
/// restore rather than silently clobber it.
#[test]
fn restore_refuses_a_file_that_changed_since_the_checkpoint() {
    let dir = workspace_with_source("checkpoint-restore-drift");
    paredit()
        .current_dir(&dir)
        .args([
            "refactor",
            "create-checkpoint",
            "--name",
            "cp1",
            "core.lisp",
        ])
        .assert()
        .success();

    let hand_edited = format!("{ORIGINAL};; a later, untracked edit\n");
    fs::write(dir.join("core.lisp"), &hand_edited).expect("simulate a manual edit");

    paredit()
        .current_dir(&dir)
        .args(["refactor", "restore-checkpoint", "--name", "cp1", "--write"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "refactor restore-checkpoint refused",
        ));

    // The refusal must not have touched the file.
    assert_eq!(read(&dir, "core.lisp"), hand_edited);
}

#[test]
fn restore_of_an_unknown_checkpoint_names_the_problem() {
    let dir = workspace_with_source("checkpoint-restore-unknown");
    paredit()
        .current_dir(&dir)
        .args(["refactor", "restore-checkpoint", "--name", "does-not-exist"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no checkpoint named"));
}

#[test]
fn delete_checkpoint_removes_it_from_the_registry() {
    let dir = workspace_with_source("checkpoint-delete");
    paredit()
        .current_dir(&dir)
        .args([
            "refactor",
            "create-checkpoint",
            "--name",
            "cp1",
            "core.lisp",
        ])
        .assert()
        .success();

    paredit()
        .current_dir(&dir)
        .args(["refactor", "delete-checkpoint", "--name", "cp1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"deleted\": true"));

    assert!(!dir.join(".paredit/checkpoints/cp1.json").exists());

    paredit()
        .current_dir(&dir)
        .args(["refactor", "delete-checkpoint", "--name", "cp1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no checkpoint named"));
}

#[test]
fn the_checkpoints_directory_can_be_overridden() {
    let dir = workspace_with_source("checkpoint-dir-override");
    paredit()
        .current_dir(&dir)
        .args([
            "refactor",
            "create-checkpoint",
            "--name",
            "cp1",
            "--checkpoints-dir",
            "custom-checkpoints",
            "core.lisp",
        ])
        .assert()
        .success();

    assert!(dir.join("custom-checkpoints/cp1.json").is_file());
    assert!(!dir.join(".paredit/checkpoints/cp1.json").exists());
}
