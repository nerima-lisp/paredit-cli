use super::*;

#[test]
fn cli_flags_not_equal() {
    let dir = fresh_temp_dir("negated-comparison-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun distinct? (a b) (not (= a b)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("negated-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"complement\": \"/=\""));
}

#[test]
fn cli_flags_null_of_comparison() {
    let dir = fresh_temp_dir("negated-comparison-report-null");
    let file = dir.join("a.lisp");
    fs::write(&file, "(null (>= p q))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("negated-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"complement\": \"<\""));
}

#[test]
fn cli_does_not_flag_three_arg_or_non_comparison() {
    let dir = fresh_temp_dir("negated-comparison-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(not (= a b c))\n(not (evenp x))\n(not flag)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("negated-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_negated_comparison_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("negated-comparison-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(not (> x y))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("negated-comparison")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "negated-comparison-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_every_complement() {
    let dir = fresh_temp_dir("negated-comparison-report-fix");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        "(not (= a b))\n(not (/= a b))\n(not (< a b))\n(not (> a b))\n(not (<= a b))\n(not (>= a b))\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("negated-comparison")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(
        fixed,
        "(/= a b)\n(= a b)\n(>= a b)\n(<= a b)\n(> a b)\n(< a b)\n"
    );
}
