use super::*;

#[test]
fn cli_reports_a_let_binding_that_shadows_a_parameter() {
    let dir = fresh_temp_dir("shadowed-binding-report");
    let lisp_file = dir.join("core.lisp");
    fs::write(&lisp_file, "(defun f (x) (let ((x 1)) (+ x 1)))\n").expect("write lisp fixture");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("shadowed-bindings")
        .arg("--output")
        .arg("json")
        .arg(&lisp_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"shadowed_count\": 1"))
        .stdout(predicate::str::contains("\"name\": \"x\""))
        .stdout(predicate::str::contains("\"outer_kind\": \"parameter\""))
        .stdout(predicate::str::contains("\"outer_label\": \"f\""));
}

#[test]
fn cli_reports_a_nested_let_that_shadows_an_outer_let() {
    let dir = fresh_temp_dir("shadowed-binding-report-nested");
    let lisp_file = dir.join("core.lisp");
    fs::write(
        &lisp_file,
        "(defun f () (let ((x 1)) (let ((x 2)) (+ x 1))))\n",
    )
    .expect("write lisp fixture");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("shadowed-bindings")
        .arg("--output")
        .arg("json")
        .arg(&lisp_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"shadowed_count\": 1"))
        .stdout(predicate::str::contains("\"outer_kind\": \"let-binding\""));
}

#[test]
fn cli_shadowed_bindings_does_not_flag_distinct_names() {
    let dir = fresh_temp_dir("shadowed-binding-report-clean");
    let lisp_file = dir.join("core.lisp");
    fs::write(&lisp_file, "(defun f (x) (let ((y 1)) (+ x y)))\n").expect("write lisp fixture");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("shadowed-bindings")
        .arg("--output")
        .arg("json")
        .arg(&lisp_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"shadowed_count\": 0"));
}

#[test]
fn cli_shadowed_bindings_fail_on_shadowed_trips_gate() {
    let dir = fresh_temp_dir("shadowed-binding-report-gate");
    let lisp_file = dir.join("core.lisp");
    fs::write(&lisp_file, "(defun f (x) (let ((x 1)) (+ x 1)))\n").expect("write lisp fixture");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("shadowed-bindings")
        .arg("--fail-on-shadowed")
        .arg(&lisp_file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "shadowed-binding-report policy failed",
        ));
}

#[test]
fn cli_shadowed_bindings_passes_gate_when_nothing_shadows() {
    let dir = fresh_temp_dir("shadowed-binding-report-gate-clean");
    let lisp_file = dir.join("core.lisp");
    fs::write(&lisp_file, "(defun f (x) (let ((y 1)) (+ x y)))\n").expect("write lisp fixture");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("shadowed-bindings")
        .arg("--fail-on-shadowed")
        .arg("--output")
        .arg("json")
        .arg(&lisp_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"shadowed_count\": 0"));
}
