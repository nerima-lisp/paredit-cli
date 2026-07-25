use super::*;

#[test]
fn cli_reports_a_symbol_exported_twice() {
    let dir = fresh_temp_dir("duplicate-export-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defpackage :app (:export #:run #:stop #:run))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-exports")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"duplicate_count\": 1"))
        .stdout(predicate::str::contains("\"symbol\": \"run\""));
}

#[test]
fn cli_does_not_flag_distinct_exports() {
    let dir = fresh_temp_dir("duplicate-export-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defpackage :app (:export #:a #:b #:c))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-exports")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"duplicate_count\": 0"));
}

#[test]
fn cli_expands_a_directory_argument() {
    let dir = fresh_temp_dir("duplicate-export-report-dir");
    let sub = dir.join("sub");
    std::fs::create_dir_all(&sub).expect("create sub dir");
    fs::write(sub.join("x.lisp"), "(defpackage :x (:export #:a #:a))\n").expect("write x.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-exports")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"duplicate_count\": 1"));
}

#[test]
fn cli_duplicate_exports_fail_on_duplicate_trips_gate() {
    let dir = fresh_temp_dir("duplicate-export-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defpackage :app (:export #:a #:a))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-exports")
        .arg("--fail-on-duplicate")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "duplicate-export-report policy failed",
        ));
}

#[test]
fn cli_duplicate_exports_passes_gate_when_clean() {
    let dir = fresh_temp_dir("duplicate-export-report-gate-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defpackage :app (:export #:a #:b))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-exports")
        .arg("--fail-on-duplicate")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"duplicate_count\": 0"));
}
