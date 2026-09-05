use super::*;

#[test]
fn cli_reports_an_if_with_identical_branches() {
    let dir = fresh_temp_dir("identical-if-branch-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if test (foo x) (foo x))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("identical-if-branches")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"if_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("(foo x)"));
}

#[test]
fn cli_finds_an_if_nested_in_a_function_body() {
    let dir = fresh_temp_dir("identical-if-branch-report-nested");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (test x) (if test (g x) (g x)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("identical-if-branches")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"));
}

#[test]
fn cli_does_not_flag_different_branches() {
    let dir = fresh_temp_dir("identical-if-branch-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if test (foo) (bar))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("identical-if-branches")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "the one two-armed `if` here
        // differs" from "no two-armed `if` at all".
        .stdout(predicate::str::contains("\"if_form_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("identical-if-branch-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn pick [test] (if test 1 1))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "identical-if-branches", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_identical_if_branches_emits_sarif() {
    let dir = fresh_temp_dir("identical-if-branch-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if test (foo x) (foo x))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "identical-if-branches", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/identical-if-branches/identical-if-branches\"",
        ))
        .stdout(predicate::str::contains(
            "if branches are identical: (foo x)",
        ));
}

#[test]
fn cli_identical_if_branches_fail_on_identical_trips_gate() {
    let dir = fresh_temp_dir("identical-if-branch-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if test a a)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("identical-if-branches")
        .arg("--fail-on-identical")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "identical-if-branch-report policy failed",
        ));
}

#[test]
fn cli_identical_if_branches_passes_gate_when_clean() {
    let dir = fresh_temp_dir("identical-if-branch-report-gate-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if test a b)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("identical-if-branches")
        .arg("--fail-on-identical")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}
