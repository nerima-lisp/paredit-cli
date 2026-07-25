use super::*;

#[test]
fn cli_flags_if_nil_else() {
    let dir = fresh_temp_dir("if-to-unless-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if ready nil (do-work))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("if-to-unless")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_does_not_flag_else_t() {
    let dir = fresh_temp_dir("if-to-unless-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if c nil t)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("if-to-unless")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_if_to_unless_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("if-to-unless-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if c nil e)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("if-to-unless")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "if-to-unless-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_to_unless() {
    let dir = fresh_temp_dir("if-to-unless-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if ready nil (do-work))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("if-to-unless")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(unless ready (do-work))\n");
}
