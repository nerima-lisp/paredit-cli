//! The blast radius `refactor apply` discloses before it is asked to write.
//!
//! Section I item I16. `cap-std` already confines writes to `--root`; what was
//! missing is a way for the caller to *see* the confinement and the file list
//! it applies to, in the dry run, without having to trust that a missing error
//! means what they hope.

use super::*;

use serde_json::Value;
use std::path::PathBuf;

const ORIGINAL: &str = "(defun old-name (x) x)\n";

fn workspace_with_manifest(label: &str) -> PathBuf {
    let dir = fresh_temp_dir(label);
    fs::write(dir.join("core.lisp"), ORIGINAL).expect("write source");
    fs::write(dir.join("untouched.lisp"), "(defun other () 1)\n").expect("write second source");

    let manifest = paredit()
        .current_dir(&dir)
        .args(["refactor", "preview", "--mode", "symbol"])
        .args(["--from", "old-name", "--to", "new-name"])
        .args(["core.lisp", "untouched.lisp", "--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    fs::write(dir.join("preview.json"), manifest).expect("write manifest");
    dir
}

fn apply_report(dir: &std::path::Path, extra: &[&str]) -> Value {
    let output = paredit()
        .current_dir(dir)
        .args(["refactor", "apply", "--manifest", "preview.json"])
        .args(extra)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).expect("apply report is JSON")
}

/// The disclosure has to be available *before* `--write`, or it cannot inform
/// the decision to pass it.
#[test]
fn a_dry_run_discloses_what_it_would_write() {
    let dir = workspace_with_manifest("scope-dry-run");
    let scope = apply_report(&dir, &[])["write_scope"].clone();

    assert_eq!(scope["target_count"], 1);
    // Canonicalized on both sides: the disclosure resolves the path so the
    // caller learns which file it is, and on macOS `/tmp` resolves through a
    // symlink to `/private/tmp`.
    assert_eq!(
        scope["targets"],
        serde_json::json!([fs::canonicalize(dir.join("core.lisp"))
            .expect("canonicalize source")
            .display()
            .to_string()])
    );
    assert_eq!(scope["confined"], false);
    assert_eq!(scope["root"], Value::Null);

    // The file that was analysed and left alone is disclosed too: "in the
    // manifest but unchanged" is a different assurance from "not considered".
    assert_eq!(scope["unchanged_count"], 1);

    assert_eq!(
        fs::read_to_string(dir.join("core.lisp")).expect("read source"),
        ORIGINAL,
        "a dry run must not write"
    );
}

#[test]
fn a_root_confined_run_names_the_root_it_is_confined_to() {
    let dir = workspace_with_manifest("scope-confined");
    let scope = apply_report(&dir, &["--root", "."])["write_scope"].clone();

    assert_eq!(scope["confined"], true);
    assert!(
        scope["root"].as_str().is_some_and(|root| !root.is_empty()),
        "a confined scope must name its root: {scope}"
    );
    assert!(
        scope["escaping_paths"]
            .as_array()
            .expect("array")
            .is_empty()
    );
}

/// The claim is re-derived from the paths rather than restated, so the field
/// exists to be empty. A non-empty one would mean the confinement and the
/// resolved paths disagree.
#[test]
fn a_confined_run_reports_no_escaping_paths() {
    let dir = workspace_with_manifest("scope-no-escape");
    let scope = apply_report(&dir, &["--root", "."])["write_scope"].clone();

    assert_eq!(scope["escaping_paths"], serde_json::json!([]));
}

/// Disclosure and enforcement are not the same thing, and the enforcement has
/// to still be there: a manifest naming a file outside the root is refused,
/// not merely reported.
#[test]
fn a_path_outside_the_root_is_refused_rather_than_disclosed() {
    let dir = workspace_with_manifest("scope-outside");
    let outside = fresh_temp_dir("scope-outside-target");
    fs::write(outside.join("stray.lisp"), ORIGINAL).expect("write stray source");

    let manifest = paredit()
        .current_dir(&outside)
        .args(["refactor", "preview", "--mode", "symbol"])
        .args(["--from", "old-name", "--to", "new-name"])
        .args(["stray.lisp", "--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    // Rewrite the manifest so its one path points outside the guarded root.
    let mut value: Value = serde_json::from_slice(&manifest).expect("manifest is JSON");
    value["files"][0]["path"] = Value::String(outside.join("stray.lisp").display().to_string());
    fs::write(
        dir.join("outside.json"),
        serde_json::to_string_pretty(&value).expect("serialize manifest"),
    )
    .expect("write manifest");

    paredit()
        .current_dir(&dir)
        .args(["refactor", "apply", "--manifest", "outside.json"])
        .args(["--root", "."])
        .assert()
        .failure()
        .stderr(predicate::str::contains("outside refactor root"));

    assert_eq!(
        fs::read_to_string(outside.join("stray.lisp")).expect("read stray source"),
        ORIGINAL,
        "a path outside the root must not be written"
    );
}

#[test]
fn the_text_output_carries_the_same_disclosure() {
    let dir = workspace_with_manifest("scope-text");

    paredit()
        .current_dir(&dir)
        .args(["refactor", "apply", "--manifest", "preview.json"])
        .args(["--root", ".", "--output", "text"])
        .assert()
        .success()
        .stdout(predicate::str::contains("write_scope_confined\ttrue"))
        .stdout(predicate::str::contains("write_scope_target_count\t1"))
        .stdout(predicate::str::contains("write_scope_target\t"));
}

/// Manifest order must not reach the disclosure, or two runs over the same
/// change produce different bytes and cannot be compared.
#[test]
fn the_disclosure_is_stable_across_runs() {
    let dir = workspace_with_manifest("scope-stable");
    let first = apply_report(&dir, &["--root", "."])["write_scope"].clone();
    let second = apply_report(&dir, &["--root", "."])["write_scope"].clone();

    assert_eq!(first, second);
}
