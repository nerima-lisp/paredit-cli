use super::*;

#[test]
fn cli_reports_an_export_never_referenced_by_a_qualified_symbol() {
    let dir = fresh_temp_dir("unused-export-report");
    let lib_file = dir.join("lib.lisp");
    let app_file = dir.join("app.lisp");
    fs::write(&lib_file, "(defpackage :lib (:export #:live #:dead))\n").expect("write lib.lisp");
    fs::write(&app_file, "(defun f () (lib:live))\n").expect("write app.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("unused-exports")
        .arg("--output")
        .arg("json")
        .arg(&lib_file)
        .arg(&app_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"unused_count\": 1"))
        .stdout(predicate::str::contains("\"dead\""));
}

#[test]
fn cli_does_not_flag_an_export_reached_by_a_double_colon_reference() {
    let dir = fresh_temp_dir("unused-export-report-double-colon");
    let lib_file = dir.join("lib.lisp");
    let app_file = dir.join("app.lisp");
    fs::write(&lib_file, "(defpackage :lib (:export #:live))\n").expect("write lib.lisp");
    fs::write(&app_file, "(defun f () (lib::live))\n").expect("write app.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("unused-exports")
        .arg("--output")
        .arg("json")
        .arg(&lib_file)
        .arg(&app_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"unused_count\": 0"));
}

#[test]
fn cli_unused_exports_fail_on_unused_trips_gate() {
    let dir = fresh_temp_dir("unused-export-report-gate");
    let lib_file = dir.join("lib.lisp");
    fs::write(&lib_file, "(defpackage :lib (:export #:dead))\n").expect("write lib.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("unused-exports")
        .arg("--fail-on-unused")
        .arg(&lib_file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "unused-export-report policy failed",
        ));
}

#[test]
fn cli_unused_exports_passes_gate_when_all_are_referenced() {
    let dir = fresh_temp_dir("unused-export-report-gate-clean");
    let lib_file = dir.join("lib.lisp");
    let app_file = dir.join("app.lisp");
    fs::write(&lib_file, "(defpackage :lib (:export #:live))\n").expect("write lib.lisp");
    fs::write(&app_file, "(defun f () (lib:live))\n").expect("write app.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("unused-exports")
        .arg("--fail-on-unused")
        .arg("--output")
        .arg("json")
        .arg(&lib_file)
        .arg(&app_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"unused_count\": 0"));
}
