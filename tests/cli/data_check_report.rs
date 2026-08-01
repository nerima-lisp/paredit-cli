//! `inspect data-check`: schema-free structural sanity checks for S-expression
//! data files. See `packages/feature/data-report/src/data_check_report/domain.rs`
//! for the unit-level coverage of the detection heuristics themselves; this
//! file exercises the command end to end, through the `paredit` binary.

use super::*;

#[test]
fn a_duplicate_alist_key_is_reported_and_fails_with_the_gate() {
    let dir = fresh_temp_dir("data-check-duplicate-alist-key");
    let file = dir.join("data.lisp");
    fs::write(&file, "((key1 . v1) (key2 . v2) (key1 . v3))\n").expect("write fixture");

    let output = paredit()
        .args(["inspect", "data-check", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).expect("data-check is JSON");
    assert_eq!(report["finding_count"], 1, "{report}");

    paredit()
        .args(["inspect", "data-check", "--fail-on-finding"])
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("policy failed"));
}

#[test]
fn an_alist_with_no_duplicates_reports_nothing() {
    let dir = fresh_temp_dir("data-check-clean-alist");
    let file = dir.join("data.lisp");
    fs::write(&file, "((key1 . v1) (key2 . v2) (key3 . v3))\n").expect("write fixture");

    let output = paredit()
        .args(["inspect", "data-check", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).expect("data-check is JSON");
    assert_eq!(report["finding_count"], 0, "{report}");
}

#[test]
fn a_well_formed_plist_reports_nothing() {
    let dir = fresh_temp_dir("data-check-clean-plist");
    let file = dir.join("data.lisp");
    fs::write(&file, "(:a 1 :b 2 :c 3)\n").expect("write fixture");

    let output = paredit()
        .args(["inspect", "data-check", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).expect("data-check is JSON");
    assert_eq!(report["finding_count"], 0, "{report}");
}

#[test]
fn a_plist_with_an_odd_trailing_keyword_is_reported() {
    let dir = fresh_temp_dir("data-check-odd-plist");
    let file = dir.join("data.lisp");
    fs::write(&file, "(:a 1 :b)\n").expect("write fixture");

    let output = paredit()
        .args(["inspect", "data-check", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).expect("data-check is JSON");
    assert_eq!(report["finding_count"], 1, "{report}");
    let kind = report["files"][0]["findings"][0]["kind"]
        .as_str()
        .expect("finding kind");
    assert_eq!(kind, "odd-length-plist");
}

#[test]
fn a_file_with_no_data_like_structure_reports_nothing_and_does_not_error() {
    let dir = fresh_temp_dir("data-check-code-file");
    let file = dir.join("code.lisp");
    fs::write(&file, "(defun add (a b) (+ a b))\n").expect("write fixture");

    let output = paredit()
        .args(["inspect", "data-check", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).expect("data-check is JSON");
    assert_eq!(report["finding_count"], 0, "{report}");
}

#[test]
fn text_output_lists_the_repeated_key() {
    let dir = fresh_temp_dir("data-check-text-output");
    let file = dir.join("data.lisp");
    fs::write(&file, "(:a 1 :a 2)\n").expect("write fixture");

    paredit()
        .args(["inspect", "data-check", "--output", "text"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("duplicate-key"))
        .stdout(predicate::str::contains(":a"));
}
