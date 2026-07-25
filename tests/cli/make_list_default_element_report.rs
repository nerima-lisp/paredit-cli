use super::*;

#[test]
fn cli_flags_initial_element_nil() {
    let dir = fresh_temp_dir("make-list-default-element-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(make-list n :initial-element nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("make-list-default-element")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_does_not_flag_non_nil() {
    let dir = fresh_temp_dir("make-list-default-element-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(make-list n :initial-element 0)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("make-list-default-element")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("make-list-default-element-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(make-list n :initial-element nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("make-list-default-element")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "make-list-default-element-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_drops_default_element() {
    let dir = fresh_temp_dir("make-list-default-element-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(make-list n :initial-element nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("make-list-default-element")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(make-list n)\n");
}
