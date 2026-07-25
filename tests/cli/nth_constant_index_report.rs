use super::*;

#[test]
fn cli_flags_nth_zero() {
    let dir = fresh_temp_dir("nth-constant-index-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun head-of (xs) (nth 0 xs))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nth-constant-index")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"ordinal\": \"first\""));
}

#[test]
fn cli_does_not_flag_large_or_variable_index() {
    let dir = fresh_temp_dir("nth-constant-index-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(nth 10 x)\n(nth i x)\n(elt x 0)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nth-constant-index")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_nth_constant_index_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("nth-constant-index-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(nth 2 row)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nth-constant-index")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "nth-constant-index-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_nth_to_ordinal() {
    let dir = fresh_temp_dir("nth-constant-index-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(nth 0 (rest pairs))\n(nth 1 row)\n(nth 9 cols)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("nth-constant-index")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(first (rest pairs))\n(second row)\n(tenth cols)\n");
}
