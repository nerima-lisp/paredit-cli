use super::*;

#[test]
fn cli_flags_a_progn_body_of_when() {
    let dir = fresh_temp_dir("redundant-body-progn-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(when ready (progn (log) (run)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-body-progn")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"implicit_progn_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"body_form_count\": 2"))
        .stdout(predicate::str::contains("\"parent\": \"when\""));
}

#[test]
fn cli_flags_a_progn_body_of_defun() {
    let dir = fresh_temp_dir("redundant-body-progn-report-defun");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (x) (progn (a) (b)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-body-progn")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"parent\": \"defun\""));
}

#[test]
fn cli_does_not_flag_single_form_or_binding_init_progns() {
    let dir = fresh_temp_dir("redundant-body-progn-report-clean");
    let file = dir.join("a.lisp");
    // Single-form progn (redundant-progn's job) and a progn as a binding init.
    fs::write(&file, "(when c (progn x))\n(let ((y (progn a b))) y)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-body-progn")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator counts the *enclosing* implicit-progn macro forms —
        // the `when` and the `let` — not the progns inside them, which is what
        // separates "two bodies looked at, both fine" from "nothing looked at".
        .stdout(predicate::str::contains("\"implicit_progn_form_count\": 2"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("redundant-body-progn-report-unmodelled");
    let file = dir.join("a.clj");
    fs::write(&file, "(when c (progn a b))\n").expect("write a.clj");

    paredit()
        .args(["inspect", "redundant-body-progn", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_redundant_body_progn_emits_sarif() {
    let dir = fresh_temp_dir("redundant-body-progn-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(when c (progn a b))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "redundant-body-progn", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/redundant-body-progn/redundant-body-progn\"",
        ))
        .stdout(predicate::str::contains(
            "progn with 2 forms is a when body; splice its forms in",
        ));
}

#[test]
fn cli_redundant_body_progn_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("redundant-body-progn-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(unless done (progn a b))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-body-progn")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "redundant-body-progn-report policy failed",
        ));
}

#[test]
fn cli_redundant_body_progn_expands_directory_inputs() {
    let dir = fresh_temp_dir("redundant-body-progn-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(defun g () (let ((x 1)) (progn (p) (q))))\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-body-progn")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"));
}
