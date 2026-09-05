use super::*;

#[test]
fn cli_flags_explicit_eql_test() {
    let dir = fresh_temp_dir("redundant-eql-test-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(find x list :test #'eql)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-eql-test")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"call_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"head\": \"find\""));
}

#[test]
fn cli_does_not_flag_custom_test_or_non_eql_head() {
    let dir = fresh_temp_dir("redundant-eql-test-report-clean");
    let file = dir.join("a.lisp");
    // A custom test, :test-not, and a non-eql-defaulting head are all left alone.
    fs::write(
        &file,
        "(find x list :test #'equal)\n(remove x l :test-not #'eql)\n(sort xs #'<)\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-eql-test")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no redundant :test in two
        // eql-defaulting calls" from "no such call at all"; `sort` is not one.
        .stdout(predicate::str::contains("\"call_form_count\": 2"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("redundant-eql-test-report-unmodelled");
    let file = dir.join("a.clj");
    fs::write(&file, "(find x list :test #'eql)\n").expect("write a.clj");

    paredit()
        .args(["inspect", "redundant-eql-test", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_redundant_eql_test_emits_sarif() {
    let dir = fresh_temp_dir("redundant-eql-test-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(find x list :test #'eql)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "redundant-eql-test", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/redundant-eql-test/redundant-eql-test\"",
        ))
        .stdout(predicate::str::contains(
            "find defaults :test to eql; the explicit :test #'eql is redundant",
        ));
}

#[test]
fn cli_redundant_eql_test_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("redundant-eql-test-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(member key items :test #'eql)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-eql-test")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "redundant-eql-test-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_deletes_the_test_pair() {
    let dir = fresh_temp_dir("redundant-eql-test-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(remove-duplicates seq :test #'eql :from-end t)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("redundant-eql-test")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(remove-duplicates seq :from-end t)\n");
}
