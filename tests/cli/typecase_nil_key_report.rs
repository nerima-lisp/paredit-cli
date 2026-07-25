use super::*;

#[test]
fn cli_flags_bare_nil_type() {
    let dir = fresh_temp_dir("typecase-nil-key-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(typecase x (nil 1) (t 2))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("typecase-nil-key")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"head\": \"typecase\""));
}

#[test]
fn cli_does_not_flag_null_type_or_quoted_nil() {
    let dir = fresh_temp_dir("typecase-nil-key-report-clean");
    let file = dir.join("a.lisp");
    // (null …) is the correct spelling; 'nil is a quoted datum; ordinary types
    // are fine.
    fs::write(
        &file,
        "(typecase x (null 1))\n(typecase y ('nil 1))\n(typecase z (integer 1) (t 2))\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("typecase-nil-key")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_typecase_nil_key_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("typecase-nil-key-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(etypecase state (nil (idle)) (integer (tick)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("typecase-nil-key")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "typecase-nil-key-report policy failed",
        ));
}

#[test]
fn cli_lint_reports_typecase_nil_key_as_an_error() {
    // The aggregate lint reports it and, being error-severity, --fail-on error trips.
    let dir = fresh_temp_dir("typecase-nil-key-report-lint");
    let file = dir.join("a.lisp");
    fs::write(&file, "(typecase x (nil 1) (t 2))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("typecase-nil-key")
        .arg("--fail-on")
        .arg("error")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("lint-report policy failed"));
}
