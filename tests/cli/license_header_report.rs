use super::*;

#[test]
fn cli_flags_a_file_with_no_leading_comment_as_missing() {
    let dir = fresh_temp_dir("license-header-report-missing");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f () 1)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("license-headers")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\": \"missing\""))
        .stdout(predicate::str::contains("\"missing_count\": 1"));
}

#[test]
fn cli_reports_a_present_header_as_present() {
    let dir = fresh_temp_dir("license-header-report-present");
    let file = dir.join("a.lisp");
    fs::write(&file, ";; Copyright 2024 Example Corp\n(defun f () 1)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("license-headers")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\": \"present\""))
        .stdout(predicate::str::contains("\"missing_count\": 0"));
}

/// Consistency is a whole-fileset question: a header that differs from the
/// majority header across the analyzed files is flagged `inconsistent`,
/// distinct from `missing`.
#[test]
fn cli_flags_a_header_that_differs_from_the_filesets_majority_as_inconsistent() {
    let dir = fresh_temp_dir("license-header-report-inconsistent");
    let a_file = dir.join("a.lisp");
    let b_file = dir.join("b.lisp");
    let c_file = dir.join("c.lisp");
    fs::write(&a_file, ";; Copyright 2024\n(defun a () 1)\n").expect("write a.lisp");
    fs::write(&b_file, ";; Copyright 2024\n(defun b () 2)\n").expect("write b.lisp");
    fs::write(&c_file, ";; Copyright 2019 Stale Corp\n(defun c () 3)\n").expect("write c.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("license-headers")
        .arg("--output")
        .arg("json")
        .arg(&a_file)
        .arg(&b_file)
        .arg(&c_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\": \"inconsistent\""))
        .stdout(predicate::str::contains("\"inconsistent_count\": 1"));
}

#[test]
fn cli_license_headers_fail_on_missing_header_trips_gate() {
    let dir = fresh_temp_dir("license-header-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f () 1)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("license-headers")
        .arg("--fail-on-missing-header")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "inspect license-headers policy failed",
        ));
}

/// The gate is armed specifically by a missing header; an inconsistent (but
/// present) header must not trip it.
#[test]
fn cli_license_headers_gate_does_not_trip_on_an_inconsistent_but_present_header() {
    let dir = fresh_temp_dir("license-header-report-gate-inconsistent-only");
    let a_file = dir.join("a.lisp");
    let b_file = dir.join("b.lisp");
    let c_file = dir.join("c.lisp");
    fs::write(&a_file, ";; Copyright 2024\n(defun a () 1)\n").expect("write a.lisp");
    fs::write(&b_file, ";; Copyright 2024\n(defun b () 2)\n").expect("write b.lisp");
    fs::write(&c_file, ";; Copyright 2019 Stale Corp\n(defun c () 3)\n").expect("write c.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("license-headers")
        .arg("--fail-on-missing-header")
        .arg("--output")
        .arg("json")
        .arg(&a_file)
        .arg(&b_file)
        .arg(&c_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\": \"inconsistent\""));
}

#[test]
fn cli_license_headers_passes_gate_when_every_file_has_a_header() {
    let dir = fresh_temp_dir("license-header-report-gate-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, ";; Copyright 2024\n(defun f () 1)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("license-headers")
        .arg("--fail-on-missing-header")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"missing_count\": 0"));
}
