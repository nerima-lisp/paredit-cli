use super::*;

#[test]
fn cli_flags_two_form_prog2() {
    let dir = fresh_temp_dir("prog2-to-progn-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(prog2 (setup) (run))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("prog2-to-progn")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_does_not_flag_three_form() {
    let dir = fresh_temp_dir("prog2-to-progn-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(prog2 a b c)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("prog2-to-progn")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_prog2_to_progn_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("prog2-to-progn-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(prog2 a b)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("prog2-to-progn")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "prog2-to-progn-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_to_progn() {
    let dir = fresh_temp_dir("prog2-to-progn-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(prog2 (setup) (run))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("prog2-to-progn")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(progn (setup) (run))\n");
}
