use super::*;

#[test]
fn cli_flags_quoted_export() {
    let dir = fresh_temp_dir("defpackage-quoted-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defpackage :app (:export 'foo))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("defpackage-quoted")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_flags_multiple() {
    let dir = fresh_temp_dir("defpackage-quoted-report-multiple");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defpackage :app (:export 'foo 'bar))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("defpackage-quoted")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 2"));
}

#[test]
fn cli_does_not_flag_unquoted() {
    let dir = fresh_temp_dir("defpackage-quoted-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defpackage :app (:export foo) (:use :cl))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("defpackage-quoted")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_defpackage_quoted_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("defpackage-quoted-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defpackage :app (:export 'foo))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("defpackage-quoted")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "defpackage-quoted-report policy failed",
        ));
}
