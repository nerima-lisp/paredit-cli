use super::*;

#[test]
fn cli_reports_a_package_declared_but_never_referenced() {
    let dir = fresh_temp_dir("unused-package-report");
    let root_file = dir.join("root.lisp");
    let app_file = dir.join("app.lisp");
    let lib_file = dir.join("lib.lisp");
    let orphan_file = dir.join("orphan.lisp");
    fs::write(&root_file, "(defpackage :root (:use :cl :app))\n").expect("write root.lisp");
    fs::write(&app_file, "(defpackage :app (:use :cl :lib))\n").expect("write app.lisp");
    fs::write(&lib_file, "(defpackage :lib (:use :cl))\n").expect("write lib.lisp");
    fs::write(&orphan_file, "(defpackage :orphan (:use :cl))\n").expect("write orphan.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("unused-packages")
        .arg("--output")
        .arg("json")
        .arg(&root_file)
        .arg(&app_file)
        .arg(&lib_file)
        .arg(&orphan_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"unused_count\": 2"))
        .stdout(predicate::str::contains("\"orphan\""))
        .stdout(predicate::str::contains("\"root\""));
}

#[test]
fn cli_does_not_flag_a_package_used_via_a_qualified_symbol() {
    let dir = fresh_temp_dir("unused-package-report-qualified");
    let lib_file = dir.join("lib.lisp");
    let app_file = dir.join("app.lisp");
    fs::write(&lib_file, "(defpackage :lib (:use :cl))\n").expect("write lib.lisp");
    fs::write(&app_file, "(defun f () (lib:public-api))\n").expect("write app.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("unused-packages")
        .arg("--output")
        .arg("json")
        .arg(&lib_file)
        .arg(&app_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"unused_count\": 0"));
}

#[test]
fn cli_unused_packages_fail_on_unused_trips_gate() {
    let dir = fresh_temp_dir("unused-package-report-gate");
    let orphan_file = dir.join("orphan.lisp");
    fs::write(&orphan_file, "(defpackage :orphan (:use :cl))\n").expect("write orphan.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("unused-packages")
        .arg("--fail-on-unused")
        .arg(&orphan_file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "unused-package-report policy failed",
        ));
}

#[test]
fn cli_unused_packages_passes_gate_when_all_are_referenced() {
    let dir = fresh_temp_dir("unused-package-report-gate-clean");
    let lib_file = dir.join("lib.lisp");
    let app_file = dir.join("app.lisp");
    fs::write(&lib_file, "(defpackage :lib (:use :cl))\n").expect("write lib.lisp");
    fs::write(&app_file, "(defun f () (lib:public-api))\n").expect("write app.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("unused-packages")
        .arg("--fail-on-unused")
        .arg("--output")
        .arg("json")
        .arg(&lib_file)
        .arg(&app_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"unused_count\": 0"));
}
