use super::*;

#[test]
fn cli_flags_typep_string() {
    let dir = fresh_temp_dir("typep-predicate-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(typep obj 'string)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("typep-predicate")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"predicate\": \"stringp\""));
}

#[test]
fn cli_does_not_flag_unknown_type() {
    let dir = fresh_temp_dir("typep-predicate-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(typep x 'fixnum)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("typep-predicate")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("typep-predicate-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(typep x 'null)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("typep-predicate")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "typep-predicate-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_to_predicate() {
    let dir = fresh_temp_dir("typep-predicate-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(typep (car x) 'cons)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("typep-predicate")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(consp (car x))\n");
}
