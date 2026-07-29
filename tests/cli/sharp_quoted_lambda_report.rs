use super::*;

#[test]
fn cli_flags_sharp_quoted_lambda() {
    let dir = fresh_temp_dir("sharp-quoted-lambda-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(mapcar #'(lambda (x) (* x x)) xs)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("sharp-quoted-lambda")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"lambda_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"));
}

#[test]
fn cli_does_not_flag_bare_lambda_or_sharp_quoted_symbol() {
    let dir = fresh_temp_dir("sharp-quoted-lambda-report-clean");
    let file = dir.join("a.lisp");
    // A bare lambda is idiomatic; #'foo is a normal function reference.
    fs::write(&file, "(mapcar (lambda (x) x) xs)\n(mapcar #'foo xs)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("sharp-quoted-lambda")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no redundant #' on the one lambda
        // here" from "no lambda at all"; `#'foo` is not a lambda form.
        .stdout(predicate::str::contains("\"lambda_form_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("sharp-quoted-lambda-report-unmodelled");
    let file = dir.join("a.clj");
    fs::write(&file, "(mapcar #'(lambda (x) x) xs)\n").expect("write a.clj");

    paredit()
        .args(["inspect", "sharp-quoted-lambda", "--output", "json"])
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
fn cli_sharp_quoted_lambda_emits_sarif() {
    let dir = fresh_temp_dir("sharp-quoted-lambda-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(mapcar #'(lambda (x) x) xs)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "sharp-quoted-lambda", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/sharp-quoted-lambda/sharp-quoted-lambda\"",
        ))
        .stdout(predicate::str::contains("#' on a lambda is redundant"));
}

#[test]
fn cli_sharp_quoted_lambda_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("sharp-quoted-lambda-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(sort xs #'(lambda (a b) (< a b)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("sharp-quoted-lambda")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "sharp-quoted-lambda-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_strips_the_sharp_quote() {
    let dir = fresh_temp_dir("sharp-quoted-lambda-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(mapcar #'(lambda (x) (* x x)) xs)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("sharp-quoted-lambda")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(mapcar (lambda (x) (* x x)) xs)\n");
}
