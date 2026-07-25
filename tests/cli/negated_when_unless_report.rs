use super::*;

#[test]
fn cli_flags_when_with_a_not_test() {
    let dir = fresh_temp_dir("negated-when-unless-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(when (not ready) (go))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("negated-when-unless")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"suggested_head\": \"unless\""));
}

#[test]
fn cli_flags_unless_with_a_null_test() {
    let dir = fresh_temp_dir("negated-when-unless-report-null");
    let file = dir.join("a.lisp");
    fs::write(&file, "(unless (null lst) (use lst))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("negated-when-unless")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"negator\": \"null\""))
        .stdout(predicate::str::contains("\"suggested_head\": \"when\""));
}

#[test]
fn cli_does_not_flag_a_plain_test() {
    let dir = fresh_temp_dir("negated-when-unless-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(when ready (go))\n(unless done (wait))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("negated-when-unless")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_does_not_flag_a_malformed_negation() {
    let dir = fresh_temp_dir("negated-when-unless-report-arity");
    let file = dir.join("a.lisp");
    fs::write(&file, "(when (not a b) c)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("negated-when-unless")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_negated_when_unless_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("negated-when-unless-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(when (not x) y)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("negated-when-unless")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "negated-when-unless-report policy failed",
        ));
}

#[test]
fn cli_negated_when_unless_expands_directory_inputs() {
    let dir = fresh_temp_dir("negated-when-unless-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(defun f () (unless (not ok) (run)))\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("negated-when-unless")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}
