use super::*;

#[test]
fn cli_reports_unused_parameters_across_definitions() {
    let dir = fresh_temp_dir("unused-parameter-report");
    let lisp_file = dir.join("core.lisp");
    fs::write(
        &lisp_file,
        "(defun f (x y) (+ x 1))\n\
         (defun g (a b) (+ a b))\n",
    )
    .expect("write lisp fixture");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("unused-parameters")
        .arg("--output")
        .arg("json")
        .arg(&lisp_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"checked_definition_count\": 2"))
        .stdout(predicate::str::contains("\"unused_parameter_count\": 1"))
        .stdout(predicate::str::contains("\"parameter_name\": \"y\""))
        .stdout(predicate::str::contains("\"definition_name\": \"f\""));
}

#[test]
fn cli_unused_parameters_ignores_underscore_convention() {
    let dir = fresh_temp_dir("unused-parameter-report-ignored");
    let lisp_file = dir.join("core.lisp");
    fs::write(&lisp_file, "(defun f (x _) (+ x 1))\n").expect("write lisp fixture");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("unused-parameters")
        .arg("--output")
        .arg("json")
        .arg(&lisp_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"unused_parameter_count\": 0"));
}

#[test]
fn cli_unused_parameters_respects_shadowing() {
    let dir = fresh_temp_dir("unused-parameter-report-shadowing");
    let lisp_file = dir.join("core.lisp");
    fs::write(&lisp_file, "(defun f (x) (let ((x 1)) (+ x 1)))\n").expect("write lisp fixture");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("unused-parameters")
        .arg("--output")
        .arg("json")
        .arg(&lisp_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"unused_parameter_count\": 1"))
        .stdout(predicate::str::contains("\"parameter_name\": \"x\""));
}

#[test]
fn cli_unused_parameters_fail_on_unused_trips_gate() {
    let dir = fresh_temp_dir("unused-parameter-report-gate");
    let lisp_file = dir.join("core.lisp");
    fs::write(&lisp_file, "(defun f (x y) (+ x 1))\n").expect("write lisp fixture");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("unused-parameters")
        .arg("--fail-on-unused")
        .arg(&lisp_file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "unused-parameter-report policy failed",
        ));
}

#[test]
fn cli_unused_parameters_passes_gate_for_fully_used_parameters() {
    let dir = fresh_temp_dir("unused-parameter-report-clean");
    let lisp_file = dir.join("core.lisp");
    fs::write(&lisp_file, "(defun f (x y) (+ x y))\n").expect("write lisp fixture");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("unused-parameters")
        .arg("--fail-on-unused")
        .arg("--output")
        .arg("json")
        .arg(&lisp_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"unused_parameter_count\": 0"));
}
