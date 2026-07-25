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
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"keyword\": \":x\""));
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
        .stdout(predicate::str::contains("\"violation_count\": 1"));
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
        .stdout(predicate::str::contains("\"violation_count\": 0"));
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
