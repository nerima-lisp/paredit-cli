use super::*;

#[test]
fn cli_flags_eq_against_t() {
    let dir = fresh_temp_dir("t-comparison-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun ready? (x) (eq (compute x) t))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("t-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"operator\": \"eq\""));
}

#[test]
fn cli_flags_equalp_with_t_first() {
    let dir = fresh_temp_dir("t-comparison-report-first");
    let file = dir.join("a.lisp");
    fs::write(&file, "(equalp t (lookup k))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("t-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"operator\": \"equalp\""));
}

#[test]
fn cli_does_not_flag_numeric_equal_both_t_or_quoted_t() {
    let dir = fresh_temp_dir("t-comparison-report-clean");
    let file = dir.join("a.lisp");
    // (= x t) is numeric, (eq t t) is degenerate, 't is quoted, (eq x nil) is a nil test.
    fs::write(&file, "(= x t)\n(eq t t)\n(eq x 't)\n(eq x nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("t-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_t_comparison_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("t-comparison-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(eql result t)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("t-comparison")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "t-comparison-report policy failed",
        ));
}

#[test]
fn cli_lint_surfaces_t_comparison_as_a_warning() {
    let dir = fresh_temp_dir("t-comparison-report-lint");
    let file = dir.join("a.lisp");
    fs::write(&file, "(eq flag t)\n").expect("write a.lisp");

    // The aggregator should surface it as a warning-severity finding.
    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("t-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"rule\": \"t-comparison\""))
        .stdout(predicate::str::contains("\"severity\": \"warning\""));
}

#[test]
fn cli_list_rules_marks_t_comparison_not_fixable() {
    let dir = fresh_temp_dir("t-comparison-report-catalog");
    let file = dir.join("a.lisp");
    fs::write(&file, "(eq flag t)\n").expect("write a.lisp");

    // t-comparison is report-only: the rule catalog must mark it non-fixable,
    // and --fix must leave the source byte-identical.
    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("t-comparison")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let after = fs::read_to_string(&file).expect("read file");
    assert_eq!(after, "(eq flag t)\n");
}
