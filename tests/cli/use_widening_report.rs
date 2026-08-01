use super::*;

#[test]
fn cli_flags_every_used_package_in_a_defpackage_use_clause() {
    let dir = fresh_temp_dir("use-widening-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defpackage :app (:use :cl :lib))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("use-widening")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 2"))
        .stdout(predicate::str::contains("\"declaring_package\": \"app\""))
        .stdout(predicate::str::contains("\"used_package\": \"cl\""))
        .stdout(predicate::str::contains("\"used_package\": \"lib\""))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_does_not_flag_import_from_as_widening() {
    let dir = fresh_temp_dir("use-widening-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defpackage :app (:import-from :lib :foo))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("use-widening")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// (`:use` widening is Common Lisp's package system, not Emacs Lisp's) must
/// be labelled rather than silently reported as clean.
#[test]
fn cli_use_widening_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("use-widening-report-unmodelled");
    let file = dir.join("a.el");
    fs::write(&file, "(defun f () 1)\n").expect("write a.el");

    paredit()
        .args(["inspect", "use-widening", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_use_widening_fail_on_use_trips_gate() {
    let dir = fresh_temp_dir("use-widening-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defpackage :app (:use :cl))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("use-widening")
        .arg("--fail-on-use")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "inspect use-widening policy failed",
        ));
}

#[test]
fn cli_use_widening_passes_gate_when_nothing_is_used() {
    let dir = fresh_temp_dir("use-widening-report-gate-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defpackage :app (:import-from :lib :foo))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("use-widening")
        .arg("--fail-on-use")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}
