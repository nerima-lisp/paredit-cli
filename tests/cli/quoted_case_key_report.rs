use super::*;

#[test]
fn cli_reports_quoted_keys() {
    let dir = fresh_temp_dir("quoted-case-key-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(case sym ('apple :fruit) ('carrot :veg) (t :x))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("quoted-case-key")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 2"))
        .stdout(predicate::str::contains("'apple"));
}

#[test]
fn cli_does_not_flag_ordinary_keys() {
    let dir = fresh_temp_dir("quoted-case-key-report-clean");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        "(case sym (apple :fruit) ((a b) :multi) (otherwise :x))\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("quoted-case-key")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_flags_an_ecase_quoted_key() {
    let dir = fresh_temp_dir("quoted-case-key-report-ecase");
    let file = dir.join("a.lisp");
    fs::write(&file, "(ecase sym ('a 1))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("quoted-case-key")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"ecase\""));
}

#[test]
fn cli_quoted_case_key_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("quoted-case-key-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(case x ((quote a) 1))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("quoted-case-key")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "quoted-case-key-report policy failed",
        ));
}

#[test]
fn cli_quoted_case_key_expands_directory_inputs() {
    let dir = fresh_temp_dir("quoted-case-key-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(defun f (x) (case x ('a 1)))\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("quoted-case-key")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}
