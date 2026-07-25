use super::*;

#[test]
fn cli_flags_aesthetic() {
    let dir = fresh_temp_dir("format-to-string-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(format nil \"~A\" value)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("format-to-string")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains(
            "\"replacement\": \"princ-to-string\"",
        ));
}

#[test]
fn cli_does_not_flag_surrounding_text() {
    let dir = fresh_temp_dir("format-to-string-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(format nil \"value: ~A\" x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("format-to-string")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_format_to_string_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("format-to-string-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(format nil \"~S\" x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("format-to-string")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "format-to-string-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_to_princ_to_string() {
    let dir = fresh_temp_dir("format-to-string-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(format nil \"~A\" (thing x))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("format-to-string")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(princ-to-string (thing x))\n");
}
