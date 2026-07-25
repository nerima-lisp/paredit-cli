use super::*;

#[test]
fn cli_flags_format_t_newline() {
    let dir = fresh_temp_dir("format-newline-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(format t \"~%\")\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("format-newline")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"format_form_count\": 1"));
}

#[test]
fn cli_does_not_flag_nil_destination() {
    let dir = fresh_temp_dir("format-newline-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(format nil \"~%\")\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("format-newline")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_format_newline_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("format-newline-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(format t \"~%\")\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("format-newline")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "format-newline-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_to_terpri() {
    let dir = fresh_temp_dir("format-newline-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(format t \"~%\")\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("format-newline")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(terpri)\n");
}
