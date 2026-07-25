use super::*;

#[test]
fn cli_reports_dead_islands_that_unused_definitions_cannot_see() {
    let dir = fresh_temp_dir("reachability-report");
    let lisp_file = dir.join("core.lisp");
    fs::write(
        &lisp_file,
        "(defun entry () (used))\n\
         (defun used () 1)\n\
         (defun island-a () (island-b))\n\
         (defun island-b () (island-a))\n\
         (entry)\n",
    )
    .expect("write lisp fixture");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("reachability")
        .arg("--output")
        .arg("json")
        .arg(&lisp_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"callable_definition_count\": 4"))
        .stdout(predicate::str::contains("\"unreachable_count\": 2"))
        .stdout(predicate::str::contains("\"name\": \"island-a\""))
        .stdout(predicate::str::contains("\"name\": \"island-b\""))
        .stdout(predicate::str::contains("\"name\": \"entry\"").not())
        .stdout(predicate::str::contains("\"name\": \"used\"").not());
}

#[test]
fn cli_reachability_fail_on_unreachable_trips_gate() {
    let dir = fresh_temp_dir("reachability-report-gate");
    let lisp_file = dir.join("core.lisp");
    fs::write(
        &lisp_file,
        "(defun island-a () (island-b))\n(defun island-b () (island-a))\n",
    )
    .expect("write lisp fixture");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("reachability")
        .arg("--fail-on-unreachable")
        .arg(&lisp_file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "reachability-report policy failed",
        ));
}

#[test]
fn cli_reachability_passes_gate_when_everything_is_reachable() {
    let dir = fresh_temp_dir("reachability-report-clean");
    let lisp_file = dir.join("core.lisp");
    fs::write(
        &lisp_file,
        "(defun main () (helper))\n(defun helper () 1)\n(main)\n",
    )
    .expect("write lisp fixture");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("reachability")
        .arg("--fail-on-unreachable")
        .arg("--output")
        .arg("json")
        .arg(&lisp_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"unreachable_count\": 0"));
}

#[test]
fn cli_reachability_spans_multiple_explicit_files() {
    let dir = fresh_temp_dir("reachability-report-multi-file");
    let a_file = dir.join("a.lisp");
    let b_file = dir.join("b.lisp");
    fs::write(&a_file, "(defun main () (helper))\n(main)\n").expect("write a.lisp");
    fs::write(&b_file, "(defun helper () 1)\n").expect("write b.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("reachability")
        .arg("--output")
        .arg("json")
        .arg(&a_file)
        .arg(&b_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"unreachable_count\": 0"));
}
