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

#[test]
fn an_el_file_with_custom_set_variables_is_auto_detected_and_flags_a_malformed_entry() {
    let dir = fresh_temp_dir("data-check-emacs-customize-auto");
    let file = dir.join("init.el");
    fs::write(&file, "(custom-set-variables\n '(foo-var))\n").expect("write fixture");

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
    assert_eq!(kind, "emacs-customize-malformed-entry");
}

#[test]
fn an_edn_file_is_auto_detected_and_flags_an_anonymous_function_shorthand() {
    let dir = fresh_temp_dir("data-check-edn-auto");
    let file = dir.join("data.edn");
    fs::write(&file, "[#(+ % 1)]\n").expect("write fixture");

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
    assert_eq!(kind, "edn-invalid-reader-macro");
}

#[test]
fn a_dot_dir_locals_el_file_is_auto_detected_and_flags_an_eval_key() {
    let dir = fresh_temp_dir("data-check-dir-locals-auto");
    let file = dir.join(".dir-locals.el");
    fs::write(&file, "((nil . ((eval . (progn (message \"hi\"))))))\n").expect("write fixture");

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
    assert_eq!(kind, "dir-locals-eval-present");
}

#[test]
fn an_explicit_format_overrides_auto_detection() {
    let dir = fresh_temp_dir("data-check-format-override");
    // `.clj`, not `.edn`, so auto-detection would land on `baseline` for
    // this file (only the `.edn` extension auto-selects the EDN detector) —
    // but `--format edn` should still run the EDN checks and flag the
    // anonymous-function shorthand. `.clj` keeps the initial read on
    // `Dialect::Clojure`, so the fixture parses cleanly before the format
    // override is even applied.
    let file = dir.join("plain.clj");
    fs::write(&file, "[#(+ % 1)]\n").expect("write fixture");

    let output = paredit()
        .args([
            "inspect",
            "data-check",
            "--format",
            "edn",
            "--output",
            "json",
        ])
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
    assert_eq!(kind, "edn-invalid-reader-macro");
}

#[test]
fn an_explicit_baseline_format_suppresses_the_emacs_customize_detector() {
    let dir = fresh_temp_dir("data-check-format-baseline-override");
    // Auto-detection would apply the emacs-customize checks (and flag the
    // odd-arity entry below); forcing `--format baseline` should skip that
    // detector and report nothing, since the baseline checks alone see no
    // duplicate keys, odd plists, or mismatched arity here.
    let file = dir.join("init.el");
    fs::write(&file, "(custom-set-variables\n '(foo-var))\n").expect("write fixture");

    let output = paredit()
        .args([
            "inspect",
            "data-check",
            "--format",
            "baseline",
            "--output",
            "json",
        ])
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).expect("data-check is JSON");
    assert_eq!(report["finding_count"], 0, "{report}");
}
