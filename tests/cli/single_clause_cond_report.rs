use super::*;

#[test]
fn cli_flags_single_clause_with_body() {
    let dir = fresh_temp_dir("single-clause-cond-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cond ((> x 0) (f x)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("single-clause-cond")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"cond_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"));
}

#[test]
fn cli_does_not_flag_multi_clause_catch_all_or_test_only() {
    let dir = fresh_temp_dir("single-clause-cond-report-clean");
    let file = dir.join("a.lisp");
    // Two clauses (a real branch), a t catch-all (a progn), and a test-only
    // clause (returns the test value) are all left alone.
    fs::write(
        &file,
        "(cond (a 1) (b 2))\n(cond (t (default)))\n(cond ((compute)))\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("single-clause-cond")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no single-clause cond among three
        // `cond` forms" from "no `cond` form at all".
        .stdout(predicate::str::contains("\"cond_form_count\": 3"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("single-clause-cond-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(cond ((> x 0) (f x)))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "single-clause-cond", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_single_clause_cond_emits_sarif() {
    let dir = fresh_temp_dir("single-clause-cond-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cond ((> x 0) (f x)))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "single-clause-cond", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/single-clause-cond/single-clause-cond\"",
        ))
        .stdout(predicate::str::contains(
            "single-clause cond with a body is just when",
        ));
}

#[test]
fn cli_single_clause_cond_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("single-clause-cond-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cond (ready (go)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("single-clause-cond")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "single-clause-cond-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_cond_as_when() {
    let dir = fresh_temp_dir("single-clause-cond-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cond ((> n 0) (step n) (recur n)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("single-clause-cond")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(when (> n 0) (step n) (recur n))\n");
}
