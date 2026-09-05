use super::*;

#[test]
fn cli_flags_append_nil() {
    let dir = fresh_temp_dir("append-nil-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(append xs nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("append-nil")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"append_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        // The list operand's span was in the old JSON and stays in it.
        .stdout(predicate::str::contains("\"list_span\""));
}

#[test]
fn cli_does_not_flag_non_nil_tail() {
    let dir = fresh_temp_dir("append-nil-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(append xs ys)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("append-nil")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "one append, not a nil-tailed one"
        // from "no append at all".
        .stdout(predicate::str::contains("\"append_form_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("append-nil-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(append xs nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("append-nil")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("append-nil-report policy failed"));
}

#[test]
fn cli_lint_fix_rewrites_to_copy_list() {
    let dir = fresh_temp_dir("append-nil-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(append (mapcar #'f ys) nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("append-nil")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(copy-list (mapcar #'f ys))\n");
}

#[test]
fn cli_labels_a_dialect_the_rule_does_not_model_append_nil() {
    let dir = fresh_temp_dir("append-nil-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn f [xs] (icollect [_ v (ipairs xs)] v))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "append-nil", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_append_nil_emits_sarif() {
    let dir = fresh_temp_dir("append-nil-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(append xs nil)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "append-nil", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/append-nil/append-nil\"",
        ))
        .stdout(predicate::str::contains(
            "append with a nil tail is a fresh copy; (append x nil) is (copy-list x)",
        ));
}
