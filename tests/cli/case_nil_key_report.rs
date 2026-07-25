use super::*;

#[test]
fn cli_flags_bare_nil_key() {
    let dir = fresh_temp_dir("case-nil-key-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(case x (nil 1) (t 2))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("case-nil-key")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"head\": \"case\""));
}

#[test]
fn cli_does_not_flag_nil_key_list_or_quoted_nil() {
    let dir = fresh_temp_dir("case-nil-key-report-clean");
    let file = dir.join("a.lisp");
    // ((nil) …) is the correct spelling; 'nil is quoted-case-key's concern;
    // ordinary keys are fine.
    fs::write(
        &file,
        "(case x ((nil) 1))\n(case y ('nil 1))\n(case z (a 1) (t 2))\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("case-nil-key")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_case_nil_key_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("case-nil-key-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(ecase state (nil (idle)) (running (tick)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("case-nil-key")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "case-nil-key-report policy failed",
        ));
}

#[test]
fn cli_lint_reports_case_nil_key_as_an_error() {
    // The aggregate lint reports it and, being error-severity, --fail-on error trips.
    let dir = fresh_temp_dir("case-nil-key-report-lint");
    let file = dir.join("a.lisp");
    fs::write(&file, "(case x (nil 1) (t 2))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("case-nil-key")
        .arg("--fail-on")
        .arg("error")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("lint-report policy failed"));
}
