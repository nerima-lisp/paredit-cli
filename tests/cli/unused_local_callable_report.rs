use super::*;

#[test]
fn cli_reports_a_flet_binding_never_called() {
    let dir = fresh_temp_dir("unused-local-callable-report");
    let lisp_file = dir.join("core.lisp");
    fs::write(
        &lisp_file,
        "(defun f (x) (flet ((helper (y) (+ y 1))) x))\n",
    )
    .expect("write lisp fixture");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("unused-local-callables")
        .arg("--output")
        .arg("json")
        .arg(&lisp_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"unused_count\": 1"))
        .stdout(predicate::str::contains("\"name\": \"helper\""))
        .stdout(predicate::str::contains("\"form_head\": \"flet\""));
}

#[test]
fn cli_does_not_flag_a_flet_binding_that_is_called() {
    let dir = fresh_temp_dir("unused-local-callable-report-clean");
    let lisp_file = dir.join("core.lisp");
    fs::write(
        &lisp_file,
        "(defun f (x) (flet ((helper (y) (+ y 1))) (helper x)))\n",
    )
    .expect("write lisp fixture");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("unused-local-callables")
        .arg("--output")
        .arg("json")
        .arg(&lisp_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"unused_count\": 0"));
}

#[test]
fn cli_labels_allows_mutual_recursion_as_usage() {
    let dir = fresh_temp_dir("unused-local-callable-report-labels");
    let lisp_file = dir.join("core.lisp");
    fs::write(
        &lisp_file,
        "(defun f (n) (labels ((odd? (x) (if (= x 0) nil (even? (- x 1)))) \
         (even? (x) (if (= x 0) t (odd? (- x 1))))) (even? n)))\n",
    )
    .expect("write lisp fixture");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("unused-local-callables")
        .arg("--output")
        .arg("json")
        .arg(&lisp_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"unused_count\": 0"));
}

#[test]
fn cli_unused_local_callables_fail_on_unused_trips_gate() {
    let dir = fresh_temp_dir("unused-local-callable-report-gate");
    let lisp_file = dir.join("core.lisp");
    fs::write(
        &lisp_file,
        "(defun f (x) (flet ((helper (y) (+ y 1))) x))\n",
    )
    .expect("write lisp fixture");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("unused-local-callables")
        .arg("--fail-on-unused")
        .arg(&lisp_file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "unused-local-callable-report policy failed",
        ));
}

#[test]
fn cli_unused_local_callables_passes_gate_when_all_are_called() {
    let dir = fresh_temp_dir("unused-local-callable-report-gate-clean");
    let lisp_file = dir.join("core.lisp");
    fs::write(
        &lisp_file,
        "(defun f (x) (flet ((helper (y) (+ y 1))) (helper x)))\n",
    )
    .expect("write lisp fixture");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("unused-local-callables")
        .arg("--fail-on-unused")
        .arg("--output")
        .arg("json")
        .arg(&lisp_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"unused_count\": 0"));
}
