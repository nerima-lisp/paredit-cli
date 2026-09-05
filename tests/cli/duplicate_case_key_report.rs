use super::*;

#[test]
fn cli_reports_a_key_repeated_across_two_clauses() {
    let dir = fresh_temp_dir("duplicate-case-key-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(case x (:a 1) (:b 2) (:a 3))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-case-keys")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"case_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"head\": \"case\""))
        .stdout(predicate::str::contains("\"key\": \":a\""))
        .stdout(predicate::str::contains("\"occurrence_count\": 2"));
}

#[test]
fn cli_finds_a_case_nested_in_a_function_body() {
    let dir = fresh_temp_dir("duplicate-case-key-report-nested");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (x) (case x (:a 1) (:a 2)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-case-keys")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"));
}

#[test]
fn cli_does_not_flag_distinct_keys() {
    let dir = fresh_temp_dir("duplicate-case-key-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(case x (:a 1) (:b 2) (otherwise 3))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-case-keys")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no repeat in one `case` form" from
        // "no `case` form at all".
        .stdout(predicate::str::contains("\"case_form_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_duplicate_case_keys_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("duplicate-case-key-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn pick [x] (case x (:a 1) (:a 2)))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "duplicate-case-keys", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_duplicate_case_keys_emits_sarif() {
    let dir = fresh_temp_dir("duplicate-case-key-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(case x (:a 1) (:b 2) (:a 3))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "duplicate-case-keys", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/duplicate-case-keys/duplicate-case-keys\"",
        ))
        .stdout(predicate::str::contains("case repeats key :a (2×)"));
}

#[test]
fn cli_duplicate_case_keys_fail_on_duplicate_trips_gate() {
    let dir = fresh_temp_dir("duplicate-case-key-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(case x (:a 1) (:a 2))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-case-keys")
        .arg("--fail-on-duplicate")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "duplicate-case-key-report policy failed",
        ));
}

#[test]
fn cli_duplicate_case_keys_passes_gate_when_all_keys_are_distinct() {
    let dir = fresh_temp_dir("duplicate-case-key-report-gate-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(case x (:a 1) (:b 2))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-case-keys")
        .arg("--fail-on-duplicate")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}
