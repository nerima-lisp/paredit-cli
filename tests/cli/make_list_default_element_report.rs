use super::*;

#[test]
fn cli_flags_initial_element_nil() {
    let dir = fresh_temp_dir("make-list-default-element-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(make-list n :initial-element nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("make-list-default-element")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"call_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"removal_span\""));
}

#[test]
fn cli_does_not_flag_non_nil() {
    let dir = fresh_temp_dir("make-list-default-element-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(make-list n :initial-element 0)\n(make-list n)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("make-list-default-element")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no restated default in two
        // `make-list` calls" from "no `make-list` call at all".
        .stdout(predicate::str::contains("\"call_form_count\": 2"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("make-list-default-element-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn build [n] (make-list n :initial-element nil))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "make-list-default-element", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_make_list_default_element_emits_sarif() {
    let dir = fresh_temp_dir("make-list-default-element-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(make-list n :initial-element nil)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "make-list-default-element", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/make-list-default-element/make-list-default-element\"",
        ))
        .stdout(predicate::str::contains(
            "explicit :initial-element nil restates make-list's default; drop it",
        ));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("make-list-default-element-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(make-list n :initial-element nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("make-list-default-element")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "make-list-default-element-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_drops_default_element() {
    let dir = fresh_temp_dir("make-list-default-element-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(make-list n :initial-element nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("make-list-default-element")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(make-list n)\n");
}
