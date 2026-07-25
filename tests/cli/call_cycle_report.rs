use super::*;

#[test]
fn cli_reports_mutual_recursion_as_a_cycle() {
    let dir = fresh_temp_dir("call-cycle-report");
    let lisp_file = dir.join("core.lisp");
    fs::write(
        &lisp_file,
        "(defun odd? (n) (if (= n 0) nil (even? (- n 1))))\n\
         (defun even? (n) (if (= n 0) t (odd? (- n 1))))\n\
         (defun main () (helper))\n\
         (defun helper () 1)\n",
    )
    .expect("write lisp fixture");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("call-cycles")
        .arg("--output")
        .arg("json")
        .arg(&lisp_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"callable_definition_count\": 4"))
        .stdout(predicate::str::contains("\"cycle_count\": 1"))
        .stdout(predicate::str::contains("\"even?\""))
        .stdout(predicate::str::contains("\"odd?\""));
}

#[test]
fn cli_does_not_flag_ordinary_self_recursion() {
    let dir = fresh_temp_dir("call-cycle-report-self");
    let lisp_file = dir.join("core.lisp");
    fs::write(
        &lisp_file,
        "(defun countdown (n) (if (= n 0) 0 (countdown (- n 1))))\n",
    )
    .expect("write lisp fixture");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("call-cycles")
        .arg("--output")
        .arg("json")
        .arg(&lisp_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"cycle_count\": 0"));
}

#[test]
fn cli_does_not_flag_a_simple_call_chain() {
    let dir = fresh_temp_dir("call-cycle-report-chain");
    let lisp_file = dir.join("core.lisp");
    fs::write(
        &lisp_file,
        "(defun main () (helper))\n(defun helper () (leaf))\n(defun leaf () 1)\n",
    )
    .expect("write lisp fixture");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("call-cycles")
        .arg("--output")
        .arg("json")
        .arg(&lisp_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"cycle_count\": 0"));
}

#[test]
fn cli_call_cycles_fail_on_cycle_trips_gate() {
    let dir = fresh_temp_dir("call-cycle-report-gate");
    let lisp_file = dir.join("core.lisp");
    fs::write(
        &lisp_file,
        "(defun odd? (n) (even? n))\n(defun even? (n) (odd? n))\n",
    )
    .expect("write lisp fixture");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("call-cycles")
        .arg("--fail-on-cycle")
        .arg(&lisp_file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("call-cycle-report policy failed"));
}

#[test]
fn cli_call_cycles_passes_gate_when_acyclic() {
    let dir = fresh_temp_dir("call-cycle-report-gate-clean");
    let lisp_file = dir.join("core.lisp");
    fs::write(
        &lisp_file,
        "(defun main () (helper))\n(defun helper () 1)\n",
    )
    .expect("write lisp fixture");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("call-cycles")
        .arg("--fail-on-cycle")
        .arg("--output")
        .arg("json")
        .arg(&lisp_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"cycle_count\": 0"));
}
