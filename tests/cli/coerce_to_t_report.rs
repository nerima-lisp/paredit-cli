use super::*;

#[test]
fn cli_flags_coerce_to_t() {
    let dir = fresh_temp_dir("coerce-to-t-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(coerce value t)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("coerce-to-t")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_does_not_flag_real_coercion() {
    let dir = fresh_temp_dir("coerce-to-t-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(coerce x 'list)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("coerce-to-t")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("coerce-to-t-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(coerce x t)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("coerce-to-t")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("coerce-to-t-report policy failed"));
}

#[test]
fn cli_lint_fix_unwraps() {
    let dir = fresh_temp_dir("coerce-to-t-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(coerce (compute x) t)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("coerce-to-t")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(compute x)\n");
}
