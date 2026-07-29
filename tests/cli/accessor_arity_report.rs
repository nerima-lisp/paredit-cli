use super::*;

#[test]
fn cli_reports_gethash_missing_the_table() {
    let dir = fresh_temp_dir("accessor-arity-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(gethash key)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("accessor-arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        // The denominator: one accessor call was scanned, and it was the bad one.
        .stdout(predicate::str::contains("\"call_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"operator\": \"gethash\""))
        .stdout(predicate::str::contains("\"expected\": \"2 or 3\""))
        .stdout(predicate::str::contains("\"argument_count\": 1"))
        .stdout(predicate::str::contains("\"min_arity\": 2"))
        .stdout(predicate::str::contains("\"max_arity\": 3"));
}

#[test]
fn cli_reports_nth_missing_the_list() {
    let dir = fresh_temp_dir("accessor-arity-report-nth");
    let file = dir.join("a.lisp");
    fs::write(&file, "(nth n)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("accessor-arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"exactly 2\""));
}

#[test]
fn cli_does_not_flag_valid_accessors() {
    let dir = fresh_temp_dir("accessor-arity-report-clean");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        "(nth i items)\n(elt seq 0)\n(gethash k tbl)\n(gethash k tbl d)\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("accessor-arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "four well-formed accessor calls"
        // from "no accessor call at all".
        .stdout(predicate::str::contains("\"call_count\": 4"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_does_not_flag_a_reader_conditional_argument() {
    let dir = fresh_temp_dir("accessor-arity-report-feature");
    let file = dir.join("a.lisp");
    fs::write(&file, "(gethash k #+sbcl a #-sbcl b)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("accessor-arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_accessor_arity_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("accessor-arity-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(gethash key)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("accessor-arity")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "accessor-arity-report policy failed",
        ));
}

#[test]
fn cli_accessor_arity_expands_directory_inputs() {
    let dir = fresh_temp_dir("accessor-arity-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(defun f (k) (when (gethash k) 1))\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("accessor-arity")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model_accessor_arity() {
    let dir = fresh_temp_dir("accessor-arity-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn get [t k] (. t k))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "accessor-arity", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

/// The envelope's interchange formats, which this report reached by moving onto
/// it. Asserted here only far enough to prove the command accepts them; their
/// content is covered once in `report_interop`.
#[test]
fn cli_accessor_arity_emits_sarif() {
    let dir = fresh_temp_dir("accessor-arity-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(gethash key)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "accessor-arity", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/accessor-arity/accessor-arity\"",
        ))
        .stdout(predicate::str::contains(
            "gethash takes 2 or 3 argument(s) but has 1",
        ));
}
