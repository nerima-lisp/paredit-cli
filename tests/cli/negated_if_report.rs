use super::*;

#[test]
fn cli_flags_negated_if() {
    let dir = fresh_temp_dir("negated-if-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun pick (ready) (if (not ready) 0 1))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("negated-if")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_does_not_flag_one_armed_or_positive_if() {
    let dir = fresh_temp_dir("negated-if-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if (not c) a)\n(if c a b)\n(if (not a b) x y)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("negated-if")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_negated_if_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("negated-if-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if (null xs) 0 (length xs))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("negated-if")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("negated-if-report policy failed"));
}

#[test]
fn cli_lint_fix_drops_negation_and_swaps_branches() {
    let dir = fresh_temp_dir("negated-if-report-fix");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        "(if (not ready) (do-a x) (do-b y))\n(if (null xs) 0 (length xs))\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("negated-if")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(
        fixed,
        "(if ready (do-b y) (do-a x))\n(if xs (length xs) 0)\n"
    );
}
