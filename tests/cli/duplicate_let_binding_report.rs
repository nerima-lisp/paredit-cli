use super::*;

#[test]
fn cli_reports_a_variable_bound_twice_in_a_parallel_let() {
    let dir = fresh_temp_dir("duplicate-let-binding-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(let ((x 1) (y 2) (x 3)) x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-let-bindings")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"duplicate_count\": 1"))
        .stdout(predicate::str::contains("\"name\": \"x\""));
}

#[test]
fn cli_finds_a_let_nested_in_a_function_body() {
    let dir = fresh_temp_dir("duplicate-let-binding-report-nested");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f () (let ((a 1) (a 2)) a))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-let-bindings")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"duplicate_count\": 1"));
}

#[test]
fn cli_does_not_flag_a_let_star_which_allows_shadowing() {
    let dir = fresh_temp_dir("duplicate-let-binding-report-let-star");
    let file = dir.join("a.lisp");
    fs::write(&file, "(let* ((x 1) (x (1+ x))) x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-let-bindings")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"duplicate_count\": 0"));
}

#[test]
fn cli_duplicate_let_bindings_fail_on_duplicate_trips_gate() {
    let dir = fresh_temp_dir("duplicate-let-binding-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(let ((x 1) (x 2)) x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-let-bindings")
        .arg("--fail-on-duplicate")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "duplicate-let-binding-report policy failed",
        ));
}

#[test]
fn cli_duplicate_let_bindings_passes_gate_when_all_names_are_distinct() {
    let dir = fresh_temp_dir("duplicate-let-binding-report-gate-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(let ((x 1) (y 2)) x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-let-bindings")
        .arg("--fail-on-duplicate")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"duplicate_count\": 0"));
}
