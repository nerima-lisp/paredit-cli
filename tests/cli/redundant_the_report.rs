use super::*;

#[test]
fn cli_flags_the_t() {
    let dir = fresh_temp_dir("redundant-the-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(the t (compute))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-the")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"the_form_count\": 1"));
}

#[test]
fn cli_does_not_flag_a_specific_type() {
    let dir = fresh_temp_dir("redundant-the-report-clean");
    let file = dir.join("a.lisp");
    // A meaningful type assertion and a wrong-arity form are left alone.
    fs::write(&file, "(the fixnum x)\n(the t a b)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-the")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_redundant_the_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("redundant-the-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(the t x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-the")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "redundant-the-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_unwraps_to_the_form() {
    let dir = fresh_temp_dir("redundant-the-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(the t (mapcar #'f xs))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("redundant-the")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(mapcar #'f xs)\n");
}
