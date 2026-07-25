use super::*;

#[test]
fn cli_flags_nthcdr_one() {
    let dir = fresh_temp_dir("nthcdr-small-index-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(nthcdr 1 items)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nthcdr-small-index")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"accessor\": \"cdr\""));
}

#[test]
fn cli_does_not_flag_zero_or_five() {
    let dir = fresh_temp_dir("nthcdr-small-index-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(nthcdr 0 x)\n(nthcdr 5 x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nthcdr-small-index")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_nthcdr_small_index_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("nthcdr-small-index-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(nthcdr 2 x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nthcdr-small-index")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "nthcdr-small-index-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_to_accessor() {
    let dir = fresh_temp_dir("nthcdr-small-index-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(nthcdr 3 (rest ys))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("nthcdr-small-index")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(cdddr (rest ys))\n");
}
