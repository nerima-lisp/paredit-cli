use super::*;

#[test]
fn cli_flags_duplicate_initarg() {
    let dir = fresh_temp_dir("duplicate-keyword-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(make-instance 'c :x 1 :x 2)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-keyword")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"call_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"keyword\": \":x\""))
        .stdout(predicate::str::contains("\"duplicate_span\""));
}

#[test]
fn cli_flags_make_hash_table_dup() {
    let dir = fresh_temp_dir("duplicate-keyword-report-hash");
    let file = dir.join("a.lisp");
    fs::write(&file, "(make-hash-table :test 'equal :test 'eq)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-keyword")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"));
}

#[test]
fn cli_does_not_flag_distinct_keywords() {
    let dir = fresh_temp_dir("duplicate-keyword-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(make-instance 'c :x 1 :y 2)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-keyword")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no repeated keyword in one
        // allowlisted call" from "no allowlisted call at all".
        .stdout(predicate::str::contains("\"call_form_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("duplicate-keyword-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(make-instance :c :x 1 :x 2)\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "duplicate-keyword", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_duplicate_keyword_emits_sarif() {
    let dir = fresh_temp_dir("duplicate-keyword-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(make-instance 'c :x 1 :x 2)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "duplicate-keyword", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/duplicate-keyword/duplicate-keyword\"",
        ))
        .stdout(predicate::str::contains(
            "keyword :x is passed more than once; the leftmost value wins",
        ));
}

#[test]
fn cli_duplicate_keyword_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("duplicate-keyword-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(make-instance 'c :x 1 :x 2)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-keyword")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "duplicate-keyword-report policy failed",
        ));
}
