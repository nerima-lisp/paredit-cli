use super::*;

#[test]
fn cli_flags_subseq_zero() {
    let dir = fresh_temp_dir("subseq-zero-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(subseq items 0)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("subseq-zero")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_does_not_flag_with_end() {
    let dir = fresh_temp_dir("subseq-zero-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(subseq seq 0 5)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("subseq-zero")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_subseq_zero_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("subseq-zero-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(subseq x 0)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("subseq-zero")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("subseq-zero-report policy failed"));
}

#[test]
fn cli_lint_fix_rewrites_to_copy_seq() {
    let dir = fresh_temp_dir("subseq-zero-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(subseq (rest xs) 0)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("subseq-zero")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(copy-seq (rest xs))\n");
}
