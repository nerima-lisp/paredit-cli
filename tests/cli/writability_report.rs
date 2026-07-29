//! `inspect writability`: does a write to this path stand a chance right now?
//!
//! The command has to be exercised through the binary. Its answer depends on
//! process-wide runtime settings (`--dry-run`) that a `OnceLock` pins on first
//! read, so a unit test can only ever see whichever value the first test in
//! the binary happened to install.

use super::*;

#[test]
fn writability_reports_a_new_file_as_writable() {
    let dir = fresh_temp_dir("writability-new");
    let file = dir.join("new.lisp");

    paredit()
        .args(["inspect", "writability", "--file"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::starts_with("writable: "));

    assert!(!file.exists(), "the probe must not create the target");
}

#[test]
fn writability_reports_an_existing_file_as_writable_without_changing_it() {
    let dir = fresh_temp_dir("writability-existing");
    let file = dir.join("source.lisp");
    fs::write(&file, "(defun f () 1)\n").expect("write source fixture");

    let output = paredit()
        .args(["inspect", "writability", "--output", "json", "--file"])
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value =
        serde_json::from_slice(&output).expect("writability report is JSON");

    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["status"], "ok");
    assert_eq!(report["writable"], true);
    assert_eq!(report["target_existed"], true);
    assert_eq!(report["reason"], serde_json::Value::Null);
    assert_eq!(
        fs::read_to_string(&file).expect("read source"),
        "(defun f () 1)\n",
        "the probe must leave the file's content alone"
    );
}

/// The command's primary case: the file itself is fine, the directory holding
/// it is not. The reason has to name that, because "the directory is read
/// only" and "someone else is mid-write" call for completely different things
/// from whoever reads it.
#[cfg(unix)]
#[test]
fn writability_reports_a_read_only_parent_directory_and_names_the_directory() {
    use std::os::unix::fs::PermissionsExt;

    let dir = fresh_temp_dir("writability-read-only-parent");
    let locked = dir.join("locked");
    fs::create_dir(&locked).expect("create read-only parent");
    let file = locked.join("source.lisp");
    fs::write(&file, "(defun f () 1)\n").expect("write source fixture");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o555))
        .expect("make the parent directory read-only");

    let assertion = paredit()
        .args(["inspect", "writability", "--output", "json", "--file"])
        .arg(&file)
        .assert()
        .failure();
    let report: serde_json::Value =
        serde_json::from_slice(&assertion.get_output().stdout).expect("writability report is JSON");

    assert_eq!(report["status"], "error");
    assert_eq!(report["writable"], false);
    assert_eq!(report["target_existed"], true);
    let reason = report["reason"].as_str().expect("a reason is reported");
    assert!(
        reason.contains("cannot create files in") && reason.contains(&locked.display().to_string()),
        "the reason must name the directory that cannot be written in: {reason}"
    );

    // Leave the directory removable for the harness.
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).expect("restore permissions");
}

#[cfg(unix)]
#[test]
fn writability_reports_a_symlinked_target_as_not_writable() {
    let dir = fresh_temp_dir("writability-symlink");
    let real = dir.join("real.lisp");
    let link = dir.join("link.lisp");
    fs::write(&real, "(defun f () 1)\n").expect("write link destination");
    std::os::unix::fs::symlink(&real, &link).expect("create symlink");

    paredit()
        .args(["inspect", "writability", "--file"])
        .arg(&link)
        .assert()
        .failure()
        .stdout(predicate::str::contains("refusing to write symlink"));
}

/// `--dry-run` promises that nothing is written to disk. The probe used to
/// take a write lock, create a real sibling file, write up to 64 MiB into it
/// and `sync_all` it — under the one flag whose whole point is that none of
/// that happens — and leave a `.paredit.cleanup` directory behind for good.
#[test]
fn writability_under_dry_run_refuses_to_probe_and_writes_nothing() {
    let dir = fresh_temp_dir("writability-dry-run");
    let file = dir.join("source.lisp");
    fs::write(&file, "(defun f () 1)\n").expect("write source fixture");

    let assertion = paredit()
        .args([
            "inspect",
            "writability",
            "--dry-run",
            "--output",
            "json",
            "--file",
        ])
        .arg(&file)
        .assert()
        .failure();
    let report: serde_json::Value =
        serde_json::from_slice(&assertion.get_output().stdout).expect("writability report is JSON");

    assert_eq!(report["writable"], false);
    let reason = report["reason"].as_str().expect("a reason is reported");
    assert!(
        reason.contains("--dry-run"),
        "the refusal must name the flag responsible: {reason}"
    );

    let mut entries = fs::read_dir(&dir)
        .expect("read test directory")
        .map(|entry| entry.expect("read directory entry").file_name())
        .collect::<Vec<_>>();
    entries.sort();
    assert_eq!(
        entries,
        [std::ffi::OsString::from("source.lisp")],
        "--dry-run must leave no lock, staging sibling or .paredit.cleanup behind"
    );
}
