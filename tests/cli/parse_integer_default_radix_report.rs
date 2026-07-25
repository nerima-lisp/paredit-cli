use super::*;

#[test]
fn cli_flags_radix_ten() {
    let dir = fresh_temp_dir("parse-integer-default-radix-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(parse-integer s :radix 10)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("parse-integer-default-radix")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_does_not_flag_non_ten() {
    let dir = fresh_temp_dir("parse-integer-default-radix-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(parse-integer s :radix 16)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("parse-integer-default-radix")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("parse-integer-default-radix-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(parse-integer s :radix 10)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("parse-integer-default-radix")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "parse-integer-default-radix-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_drops_default_radix() {
    let dir = fresh_temp_dir("parse-integer-default-radix-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(parse-integer s :radix 10)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("parse-integer-default-radix")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(parse-integer s)\n");
}
