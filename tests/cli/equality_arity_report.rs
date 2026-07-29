use super::*;

#[test]
fn cli_reports_too_few_arguments() {
    let dir = fresh_temp_dir("equality-arity-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(eq x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("equality-arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"call_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"eq\""))
        .stdout(predicate::str::contains("\"argument_count\": 1"));
}

#[test]
fn cli_reports_too_many_arguments() {
    let dir = fresh_temp_dir("equality-arity-report-many");
    let file = dir.join("a.lisp");
    fs::write(&file, "(equal a b c)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("equality-arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"argument_count\": 3"));
}

#[test]
fn cli_does_not_flag_binary_or_variadic_comparisons() {
    let dir = fresh_temp_dir("equality-arity-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(eq a b)\n(= x y z)\n(< n)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("equality-arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "the one equality call is binary"
        // from "there is no equality call"; `=` and `<` are variadic and never
        // counted.
        .stdout(predicate::str::contains("\"call_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_does_not_flag_a_reader_conditional_argument() {
    let dir = fresh_temp_dir("equality-arity-report-feature");
    let file = dir.join("a.lisp");
    fs::write(&file, "(eql p #+sbcl q #-sbcl r)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("equality-arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // Skipped outright, so it is not in the denominator either.
        .stdout(predicate::str::contains("\"call_count\": 0"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("equality-arity-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn same? [a b] (= a b))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "equality-arity", "--output", "json"])
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
fn cli_equality_arity_emits_sarif() {
    let dir = fresh_temp_dir("equality-arity-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(eq x)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "equality-arity", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        // The predicate is the finding's kind, so a consumer can select the
        // misarity calls of one of the four by rule id.
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/equality-arity/eq\"",
        ))
        .stdout(predicate::str::contains(
            "eq takes exactly 2 arguments but has 1",
        ));
}

#[test]
fn cli_equality_arity_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("equality-arity-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(eq x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("equality-arity")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "equality-arity-report policy failed",
        ));
}

#[test]
fn cli_equality_arity_expands_directory_inputs() {
    let dir = fresh_temp_dir("equality-arity-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(defun f (x) (when (eq x) 1))\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("equality-arity")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"));
}
