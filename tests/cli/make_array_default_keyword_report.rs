use super::*;

#[test]
fn cli_flags_adjustable_nil() {
    let dir = fresh_temp_dir("make-array-default-keyword-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(make-array n :adjustable nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("make-array-default-keyword")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_does_not_flag_non_nil() {
    let dir = fresh_temp_dir("make-array-default-keyword-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(make-array n :adjustable t)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("make-array-default-keyword")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("make-array-default-keyword-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(make-array n :adjustable nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("make-array-default-keyword")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "make-array-default-keyword-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_drops_default_keyword() {
    let dir = fresh_temp_dir("make-array-default-keyword-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(make-array n :adjustable nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("make-array-default-keyword")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(make-array n)\n");
}
