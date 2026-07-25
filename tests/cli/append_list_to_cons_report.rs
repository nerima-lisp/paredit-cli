use super::*;

#[test]
fn cli_flags_append_singleton() {
    let dir = fresh_temp_dir("append-list-to-cons-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(append (list x) rest)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("append-list-to-cons")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"append_form_count\": 1"));
}

#[test]
fn cli_does_not_flag_multi_element() {
    let dir = fresh_temp_dir("append-list-to-cons-report-multi");
    let file = dir.join("a.lisp");
    fs::write(&file, "(append (list x y) rest)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("append-list-to-cons")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_append_list_to_cons_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("append-list-to-cons-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(append (list x) rest)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("append-list-to-cons")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "append-list-to-cons-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_to_cons() {
    let dir = fresh_temp_dir("append-list-to-cons-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(append (list (car a)) (cdr b))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("append-list-to-cons")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(cons (car a) (cdr b))\n");
}
