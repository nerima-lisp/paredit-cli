use super::*;

#[test]
fn cli_flags_funcall_of_lambda() {
    let dir = fresh_temp_dir("funcall-lambda-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(funcall (lambda (x) (* x x)) 5)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("funcall-lambda")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"funcall_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"));
}

#[test]
fn cli_does_not_flag_sharp_quoted_callees() {
    let dir = fresh_temp_dir("funcall-lambda-report-clean");
    let file = dir.join("a.lisp");
    // #'symbol is redundant-funcall's job; #'(lambda …) has an illegal-in-operator
    // prefix; a variable callee is genuine.
    fs::write(
        &file,
        "(funcall #'foo 1 2)\n(funcall #'(lambda (x) x) 5)\n(funcall fn 1)\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("funcall-lambda")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no lambda callee in three
        // `funcall` forms" from "no `funcall` form at all".
        .stdout(predicate::str::contains("\"funcall_form_count\": 3"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_funcall_lambda_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("funcall-lambda-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "((fn [x] (* x x)) 5)\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "funcall-lambda", "--output", "json"])
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
fn cli_funcall_lambda_emits_sarif() {
    let dir = fresh_temp_dir("funcall-lambda-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(funcall (lambda (x) (* x x)) 5)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "funcall-lambda", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/funcall-lambda/funcall-lambda\"",
        ))
        .stdout(predicate::str::contains(
            "funcall of a lambda applies it directly",
        ));
}

#[test]
fn cli_funcall_lambda_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("funcall-lambda-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(funcall (lambda () (run)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("funcall-lambda")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "funcall-lambda-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_applies_the_lambda_directly() {
    let dir = fresh_temp_dir("funcall-lambda-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(funcall (lambda (x) (* x x)) 5)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("funcall-lambda")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "((lambda (x) (* x x)) 5)\n");
}
