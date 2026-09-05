use super::*;

#[test]
fn cli_flags_from_end_nil() {
    let dir = fresh_temp_dir("redundant-from-end-nil-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(find x seq :from-end nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-from-end-nil")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"call_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"head\": \"find\""))
        // The fix input this report has always published; a consumer scripting
        // the deletion around the command depends on it.
        .stdout(predicate::str::contains("\"removal_span\""));
}

#[test]
fn cli_does_not_flag_from_end_t() {
    let dir = fresh_temp_dir("redundant-from-end-nil-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(find x seq :from-end t)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-from-end-nil")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no redundant :from-end in one
        // call" from "no such call at all".
        .stdout(predicate::str::contains("\"call_form_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("redundant-from-end-nil-report-unmodelled");
    let file = dir.join("a.clj");
    fs::write(&file, "(find x seq :from-end nil)\n").expect("write a.clj");

    paredit()
        .args(["inspect", "redundant-from-end-nil", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_redundant_from_end_nil_emits_sarif() {
    let dir = fresh_temp_dir("redundant-from-end-nil-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(find x seq :from-end nil)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "redundant-from-end-nil", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/redundant-from-end-nil/redundant-from-end-nil\"",
        ))
        .stdout(predicate::str::contains(
            "find :from-end defaults to nil; drop the explicit :from-end nil",
        ));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("redundant-from-end-nil-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(find x seq :from-end nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-from-end-nil")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "redundant-from-end-nil-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_drops_from_end() {
    let dir = fresh_temp_dir("redundant-from-end-nil-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(remove x seq :from-end nil :count 3)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("redundant-from-end-nil")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(remove x seq :count 3)\n");
}
