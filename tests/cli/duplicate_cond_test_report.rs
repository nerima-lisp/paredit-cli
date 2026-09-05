use super::*;

#[test]
fn cli_reports_a_repeated_test_expression() {
    let dir = fresh_temp_dir("duplicate-cond-test-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cond ((foo) 1) ((bar) 2) ((foo) 3))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-cond-tests")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"cond_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"occurrence_count\": 2"))
        .stdout(predicate::str::contains("(foo)"));
}

#[test]
fn cli_finds_a_cond_nested_in_a_function_body() {
    let dir = fresh_temp_dir("duplicate-cond-test-report-nested");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (x) (cond ((p x) 1) ((p x) 2)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-cond-tests")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"));
}

#[test]
fn cli_does_not_flag_distinct_tests() {
    let dir = fresh_temp_dir("duplicate-cond-test-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cond ((foo) 1) ((bar) 2) (t 3))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-cond-tests")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no repeated test in one `cond`"
        // from "no `cond` form at all".
        .stdout(predicate::str::contains("\"cond_form_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("duplicate-cond-test-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn pick [] (cond ((foo) 1) ((foo) 2)))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "duplicate-cond-tests", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_duplicate_cond_tests_emits_sarif() {
    let dir = fresh_temp_dir("duplicate-cond-test-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cond ((foo) 1) ((foo) 2))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "duplicate-cond-tests", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/duplicate-cond-tests/duplicate-cond-tests\"",
        ))
        .stdout(predicate::str::contains("cond repeats test (foo)"));
}

#[test]
fn cli_duplicate_cond_tests_fail_on_duplicate_trips_gate() {
    let dir = fresh_temp_dir("duplicate-cond-test-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cond ((foo) 1) ((foo) 2))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-cond-tests")
        .arg("--fail-on-duplicate")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "duplicate-cond-test-report policy failed",
        ));
}

#[test]
fn cli_duplicate_cond_tests_passes_gate_when_all_tests_are_distinct() {
    let dir = fresh_temp_dir("duplicate-cond-test-report-gate-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cond ((foo) 1) ((bar) 2))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-cond-tests")
        .arg("--fail-on-duplicate")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}
