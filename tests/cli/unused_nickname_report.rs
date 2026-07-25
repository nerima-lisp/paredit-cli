use super::*;

#[test]
fn cli_reports_a_nickname_never_used_as_a_qualifier() {
    let dir = fresh_temp_dir("unused-nickname-report");
    let lib_file = dir.join("lib.lisp");
    let app_file = dir.join("app.lisp");
    fs::write(&lib_file, "(defpackage :lib (:nicknames :l :ll))\n").expect("write lib.lisp");
    fs::write(&app_file, "(defun f () (l:public-api))\n").expect("write app.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("unused-nicknames")
        .arg("--output")
        .arg("json")
        .arg(&lib_file)
        .arg(&app_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"unused_count\": 1"))
        .stdout(predicate::str::contains("\"ll\""));
}

#[test]
fn cli_does_not_flag_a_nickname_used_in_a_use_clause() {
    let dir = fresh_temp_dir("unused-nickname-report-use");
    let lib_file = dir.join("lib.lisp");
    let app_file = dir.join("app.lisp");
    fs::write(&lib_file, "(defpackage :lib (:nicknames :l))\n").expect("write lib.lisp");
    fs::write(&app_file, "(defpackage :app (:use :l))\n").expect("write app.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("unused-nicknames")
        .arg("--output")
        .arg("json")
        .arg(&lib_file)
        .arg(&app_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"unused_count\": 0"));
}

#[test]
fn cli_unused_nicknames_fail_on_unused_trips_gate() {
    let dir = fresh_temp_dir("unused-nickname-report-gate");
    let lib_file = dir.join("lib.lisp");
    fs::write(&lib_file, "(defpackage :lib (:nicknames :l))\n").expect("write lib.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("unused-nicknames")
        .arg("--fail-on-unused")
        .arg(&lib_file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "unused-nickname-report policy failed",
        ));
}

#[test]
fn cli_unused_nicknames_passes_gate_when_all_are_referenced() {
    let dir = fresh_temp_dir("unused-nickname-report-gate-clean");
    let lib_file = dir.join("lib.lisp");
    let app_file = dir.join("app.lisp");
    fs::write(&lib_file, "(defpackage :lib (:nicknames :l))\n").expect("write lib.lisp");
    fs::write(&app_file, "(defun f () (l:public-api))\n").expect("write app.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("unused-nicknames")
        .arg("--fail-on-unused")
        .arg("--output")
        .arg("json")
        .arg(&lib_file)
        .arg(&app_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"unused_count\": 0"));
}
