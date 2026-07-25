use super::*;

#[test]
fn cli_reports_an_in_package_form_naming_an_undeclared_package() {
    let dir = fresh_temp_dir("undefined-package-report");
    let typo_file = dir.join("typo.lisp");
    fs::write(&typo_file, "(in-package :aap)\n").expect("write typo.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("undefined-packages")
        .arg("--output")
        .arg("json")
        .arg(&typo_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"undefined_count\": 1"))
        .stdout(predicate::str::contains("\"aap\""));
}

#[test]
fn cli_does_not_flag_a_declared_or_standard_package() {
    let dir = fresh_temp_dir("undefined-package-report-clean");
    let app_file = dir.join("app.lisp");
    let user_file = dir.join("user.lisp");
    fs::write(
        &app_file,
        "(defpackage :app (:use :cl))\n(in-package :app)\n",
    )
    .expect("write app.lisp");
    fs::write(&user_file, "(in-package :cl-user)\n").expect("write user.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("undefined-packages")
        .arg("--output")
        .arg("json")
        .arg(&app_file)
        .arg(&user_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"undefined_count\": 0"));
}

#[test]
fn cli_undefined_packages_fail_on_undefined_trips_gate() {
    let dir = fresh_temp_dir("undefined-package-report-gate");
    let typo_file = dir.join("typo.lisp");
    fs::write(&typo_file, "(in-package :aap)\n").expect("write typo.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("undefined-packages")
        .arg("--fail-on-undefined")
        .arg(&typo_file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "undefined-package-report policy failed",
        ));
}

#[test]
fn cli_undefined_packages_passes_gate_when_all_are_declared() {
    let dir = fresh_temp_dir("undefined-package-report-gate-clean");
    let app_file = dir.join("app.lisp");
    fs::write(
        &app_file,
        "(defpackage :app (:use :cl))\n(in-package :app)\n",
    )
    .expect("write app.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("undefined-packages")
        .arg("--fail-on-undefined")
        .arg("--output")
        .arg("json")
        .arg(&app_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"undefined_count\": 0"));
}
