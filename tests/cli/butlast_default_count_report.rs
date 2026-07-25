use super::*;

#[test]
fn cli_flags_explicit_one() {
    let dir = fresh_temp_dir("butlast-default-count-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(butlast xs 1)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("butlast-default-count")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_does_not_flag_bare() {
    let dir = fresh_temp_dir("butlast-default-count-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(butlast xs)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("butlast-default-count")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("butlast-default-count-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(butlast xs 1)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("butlast-default-count")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "butlast-default-count-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_drops_default_count() {
    let dir = fresh_temp_dir("butlast-default-count-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(butlast xs 1)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("butlast-default-count")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(butlast xs)\n");
}
