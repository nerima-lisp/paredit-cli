use super::*;

#[test]
fn cli_reports_a_one_element_spec() {
    let dir = fresh_temp_dir("malformed-iteration-spec-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(dolist (x) (print x))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("malformed-iteration-spec")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"iteration_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"element_count\": 1"))
        .stdout(predicate::str::contains("\"head\": \"dolist\""))
        .stdout(predicate::str::contains("\"spec\": \"(x)\""));
}

#[test]
fn cli_does_not_flag_valid_specs() {
    let dir = fresh_temp_dir("malformed-iteration-spec-report-clean");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        "(dolist (x items) (print x))\n(dotimes (i 10 done) (print i))\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("malformed-iteration-spec")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "both specs here are well-formed"
        // from "no dolist/dotimes at all".
        .stdout(predicate::str::contains("\"iteration_form_count\": 2"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_does_not_flag_a_feature_conditional_spec() {
    let dir = fresh_temp_dir("malformed-iteration-spec-report-feature");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        "(dotimes (i #+sbcl n1 #-sbcl n2 result) (print i))\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("malformed-iteration-spec")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_flags_a_non_list_spec() {
    let dir = fresh_temp_dir("malformed-iteration-spec-report-nonlist");
    let file = dir.join("a.lisp");
    fs::write(&file, "(dolist x (print x))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("malformed-iteration-spec")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_malformed_iteration_spec_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("malformed-iteration-spec-report-unmodelled");
    let file = dir.join("a.clj");
    fs::write(&file, "(dolist (x) (print x))\n").expect("write a.clj");

    paredit()
        .args(["inspect", "malformed-iteration-spec", "--output", "json"])
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
fn cli_malformed_iteration_spec_emits_sarif() {
    let dir = fresh_temp_dir("malformed-iteration-spec-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(dolist (x) (print x))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "malformed-iteration-spec", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/malformed-iteration-spec/malformed-iteration-spec\"",
        ))
        .stdout(predicate::str::contains(
            "dolist spec (x) must be (var form [result])",
        ));
}

#[test]
fn cli_malformed_iteration_spec_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("malformed-iteration-spec-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(dotimes (i n r extra) (print i))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("malformed-iteration-spec")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "malformed-iteration-spec-report policy failed",
        ));
}

#[test]
fn cli_malformed_iteration_spec_expands_directory_inputs() {
    let dir = fresh_temp_dir("malformed-iteration-spec-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(defun f (xs) (dolist (x) (print x)))\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("malformed-iteration-spec")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"));
}
