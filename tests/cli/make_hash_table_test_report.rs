use super::*;

#[test]
fn cli_flags_eql_test() {
    let dir = fresh_temp_dir("make-hash-table-test-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(make-hash-table :test 'eql)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("make-hash-table-test")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains(
            "\"make_hash_table_form_count\": 1",
        ))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"removal_span\""));
}

#[test]
fn cli_does_not_flag_custom_test() {
    let dir = fresh_temp_dir("make-hash-table-test-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(make-hash-table :test 'equal)\n(make-hash-table)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("make-hash-table-test")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no redundant :test in two
        // `make-hash-table` calls" from "no `make-hash-table` call at all".
        .stdout(predicate::str::contains(
            "\"make_hash_table_form_count\": 2",
        ))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("make-hash-table-test-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn build [] (make-hash-table :test 'eql))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "make-hash-table-test", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_make_hash_table_test_emits_sarif() {
    let dir = fresh_temp_dir("make-hash-table-test-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(make-hash-table :test 'eql)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "make-hash-table-test", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/make-hash-table-test/make-hash-table-test\"",
        ))
        .stdout(predicate::str::contains(
            "the make-hash-table :test defaults to eql; drop the explicit :test 'eql",
        ));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("make-hash-table-test-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(make-hash-table :test 'eql)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("make-hash-table-test")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "make-hash-table-test-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_drops_test() {
    let dir = fresh_temp_dir("make-hash-table-test-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(make-hash-table :size 16 :test 'eql)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("make-hash-table-test")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(make-hash-table :size 16)\n");
}
