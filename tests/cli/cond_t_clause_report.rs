use super::*;

#[test]
fn cli_flags_single_t_clause() {
    let dir = fresh_temp_dir("cond-t-clause-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cond (t a b))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("cond-t-clause")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"cond_form_count\": 1"));
}

#[test]
fn cli_does_not_flag_multi_clause() {
    let dir = fresh_temp_dir("cond-t-clause-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cond (p a) (t b))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("cond-t-clause")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_cond_t_clause_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("cond-t-clause-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cond (t x))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("cond-t-clause")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "cond-t-clause-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_to_progn() {
    let dir = fresh_temp_dir("cond-t-clause-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cond (t (a) (b)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("cond-t-clause")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(progn (a) (b))\n");
}
