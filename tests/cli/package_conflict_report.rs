use super::*;

#[test]
fn cli_reports_a_nickname_colliding_with_another_packages_primary_name() {
    let dir = fresh_temp_dir("package-conflict-report");
    let a_file = dir.join("a.lisp");
    let b_file = dir.join("b.lisp");
    fs::write(&a_file, "(defpackage :util (:nicknames :app))\n").expect("write a.lisp");
    fs::write(&b_file, "(defpackage :app (:use :cl))\n").expect("write b.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("package-conflicts")
        .arg("--output")
        .arg("json")
        .arg(&a_file)
        .arg(&b_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"conflict_count\": 1"))
        .stdout(predicate::str::contains("\"app\""));
}

#[test]
fn cli_does_not_flag_distinct_packages_with_distinct_identities() {
    let dir = fresh_temp_dir("package-conflict-report-clean");
    let a_file = dir.join("a.lisp");
    let b_file = dir.join("b.lisp");
    fs::write(&a_file, "(defpackage :util (:nicknames :u))\n").expect("write a.lisp");
    fs::write(&b_file, "(defpackage :app (:use :cl))\n").expect("write b.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("package-conflicts")
        .arg("--output")
        .arg("json")
        .arg(&a_file)
        .arg(&b_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"conflict_count\": 0"));
}

#[test]
fn cli_package_conflicts_fail_on_conflict_trips_gate() {
    let dir = fresh_temp_dir("package-conflict-report-gate");
    let a_file = dir.join("a.lisp");
    let b_file = dir.join("b.lisp");
    fs::write(&a_file, "(defpackage :app (:use :cl))\n").expect("write a.lisp");
    fs::write(&b_file, "(defpackage :app (:use :cl))\n").expect("write b.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("package-conflicts")
        .arg("--fail-on-conflict")
        .arg(&a_file)
        .arg(&b_file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "package-conflict-report policy failed",
        ));
}

#[test]
fn cli_package_conflicts_passes_gate_when_all_identities_are_distinct() {
    let dir = fresh_temp_dir("package-conflict-report-gate-clean");
    let a_file = dir.join("a.lisp");
    let b_file = dir.join("b.lisp");
    fs::write(&a_file, "(defpackage :util (:use :cl))\n").expect("write a.lisp");
    fs::write(&b_file, "(defpackage :app (:use :cl))\n").expect("write b.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("package-conflicts")
        .arg("--fail-on-conflict")
        .arg("--output")
        .arg("json")
        .arg(&a_file)
        .arg(&b_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"conflict_count\": 0"));
}
