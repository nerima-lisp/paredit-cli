use super::*;

#[test]
fn cli_flags_a_progn_wrapping_a_single_form() {
    let dir = fresh_temp_dir("redundant-progn-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f () (progn (compute)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-progn")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"progn_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"body_form_count\": 1"));
}

#[test]
fn cli_flags_an_empty_progn() {
    let dir = fresh_temp_dir("redundant-progn-report-empty");
    let file = dir.join("a.lisp");
    fs::write(&file, "(progn)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-progn")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"body_form_count\": 0"));
}

#[test]
fn cli_does_not_flag_a_multi_form_progn() {
    let dir = fresh_temp_dir("redundant-progn-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(progn (setup) (run) (teardown))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-progn")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "this progn sequences something"
        // from "no progn form at all".
        .stdout(predicate::str::contains("\"progn_form_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_does_not_flag_a_lone_reader_conditional_body() {
    let dir = fresh_temp_dir("redundant-progn-report-feature");
    let file = dir.join("a.lisp");
    fs::write(&file, "(progn #+sbcl (sb-specific))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-progn")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("redundant-progn-report-unmodelled");
    let file = dir.join("a.clj");
    fs::write(&file, "(progn x)\n").expect("write a.clj");

    paredit()
        .args(["inspect", "redundant-progn", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_redundant_progn_emits_sarif() {
    let dir = fresh_temp_dir("redundant-progn-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(progn x)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "redundant-progn", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/redundant-progn/redundant-progn\"",
        ))
        .stdout(predicate::str::contains(
            "redundant progn: progn wraps a single form; it is equivalent to that form",
        ));
}

#[test]
fn cli_redundant_progn_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("redundant-progn-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(progn x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-progn")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "redundant-progn-report policy failed",
        ));
}

#[test]
fn cli_redundant_progn_expands_directory_inputs() {
    let dir = fresh_temp_dir("redundant-progn-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(when t (progn (only)))\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-progn")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"));
}
