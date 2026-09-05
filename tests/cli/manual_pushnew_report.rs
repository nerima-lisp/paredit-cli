use super::*;

#[test]
fn cli_flags_manual_pushnew() {
    let dir = fresh_temp_dir("manual-pushnew-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun note (k) (setf keys (adjoin k keys)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("manual-pushnew")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"assignment_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"kind\": \"manual-pushnew\""));
}

#[test]
fn cli_does_not_flag_adjoin_element_or_other_variable() {
    let dir = fresh_temp_dir("manual-pushnew-report-clean");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        "(setf xs (adjoin xs other))\n(setf xs (adjoin item ys))\n(setf (slot o) (adjoin item (slot o)))\n(setf xs (cons item xs))\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("manual-pushnew")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "none of these four assignments is
        // a hand-written pushnew" from "there is no assignment at all".
        .stdout(predicate::str::contains("\"assignment_form_count\": 4"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("manual-pushnew-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn note [k] (setf keys (adjoin k keys)))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "manual-pushnew", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_manual_pushnew_emits_sarif() {
    let dir = fresh_temp_dir("manual-pushnew-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setf seen (adjoin k seen))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "manual-pushnew", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/manual-pushnew/manual-pushnew\"",
        ))
        .stdout(predicate::str::contains(
            "setf adjoins onto a variable; use pushnew",
        ));
}

#[test]
fn cli_manual_pushnew_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("manual-pushnew-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setf seen (adjoin k seen))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("manual-pushnew")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "manual-pushnew-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_manual_pushnew_passing_keywords_through() {
    let dir = fresh_temp_dir("manual-pushnew-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setf seen (adjoin k seen :test #'equal))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("manual-pushnew")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(pushnew k seen :test #'equal)\n");
}
