use super::*;

#[test]
fn cli_flags_list_star_nil() {
    let dir = fresh_temp_dir("list-star-nil-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(list* a b nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("list-star-nil")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_does_not_flag_non_nil_tail() {
    let dir = fresh_temp_dir("list-star-nil-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(list* a b)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("list-star-nil")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("list-star-nil-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(list* a b nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("list-star-nil")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "list-star-nil-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_to_list() {
    let dir = fresh_temp_dir("list-star-nil-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(list* a b nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("list-star-nil")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(list a b)\n");
}
