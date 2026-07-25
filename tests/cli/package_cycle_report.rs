use super::*;

#[test]
fn cli_reports_a_circular_use_between_two_packages() {
    let dir = fresh_temp_dir("package-cycle-report");
    let app_file = dir.join("app.lisp");
    let lib_file = dir.join("lib.lisp");
    fs::write(&app_file, "(defpackage :app (:use :cl :lib))\n").expect("write app.lisp");
    fs::write(&lib_file, "(defpackage :lib (:use :cl :app))\n").expect("write lib.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("package-cycles")
        .arg("--output")
        .arg("json")
        .arg(&app_file)
        .arg(&lib_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"cycle_count\": 1"))
        .stdout(predicate::str::contains("\"app\""))
        .stdout(predicate::str::contains("\"lib\""));
}

#[test]
fn cli_does_not_flag_a_simple_use_chain() {
    let dir = fresh_temp_dir("package-cycle-report-chain");
    let app_file = dir.join("app.lisp");
    let util_file = dir.join("util.lisp");
    fs::write(&app_file, "(defpackage :app (:use :cl :util))\n").expect("write app.lisp");
    fs::write(&util_file, "(defpackage :util (:use :cl))\n").expect("write util.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("package-cycles")
        .arg("--output")
        .arg("json")
        .arg(&app_file)
        .arg(&util_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"cycle_count\": 0"));
}

#[test]
fn cli_package_cycles_fail_on_cycle_trips_gate() {
    let dir = fresh_temp_dir("package-cycle-report-gate");
    let app_file = dir.join("app.lisp");
    let lib_file = dir.join("lib.lisp");
    fs::write(&app_file, "(defpackage :app (:use :lib))\n").expect("write app.lisp");
    fs::write(&lib_file, "(defpackage :lib (:use :app))\n").expect("write lib.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("package-cycles")
        .arg("--fail-on-cycle")
        .arg(&app_file)
        .arg(&lib_file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "package-cycle-report policy failed",
        ));
}

#[test]
fn cli_package_cycles_passes_gate_when_acyclic() {
    let dir = fresh_temp_dir("package-cycle-report-gate-clean");
    let app_file = dir.join("app.lisp");
    fs::write(&app_file, "(defpackage :app (:use :cl))\n").expect("write app.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("package-cycles")
        .arg("--fail-on-cycle")
        .arg("--output")
        .arg("json")
        .arg(&app_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"cycle_count\": 0"));
}
