use super::*;

#[test]
fn cli_flags_double_reverse() {
    let dir = fresh_temp_dir("double-reverse-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(reverse (reverse xs))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("double-reverse")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"reverse_form_count\": 2"));
}

#[test]
fn cli_does_not_flag_single_reverse_or_nreverse() {
    let dir = fresh_temp_dir("double-reverse-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(reverse xs)\n(nreverse (nreverse xs))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("double-reverse")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_double_reverse_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("double-reverse-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(reverse (reverse xs))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("double-reverse")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "double-reverse-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_to_copy_seq() {
    let dir = fresh_temp_dir("double-reverse-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(reverse (reverse (mapcar #'f ys)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("double-reverse")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(copy-seq (mapcar #'f ys))\n");
}
