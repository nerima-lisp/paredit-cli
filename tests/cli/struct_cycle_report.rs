use super::*;

#[test]
fn cli_reports_a_circular_include_between_two_structs() {
    let dir = fresh_temp_dir("struct-cycle-report");
    let a_file = dir.join("a.lisp");
    let b_file = dir.join("b.lisp");
    fs::write(&a_file, "(defstruct (a (:include b)) (x 0))\n").expect("write a.lisp");
    fs::write(&b_file, "(defstruct (b (:include a)) (y 0))\n").expect("write b.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("struct-cycles")
        .arg("--output")
        .arg("json")
        .arg(&a_file)
        .arg(&b_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"cycle_count\": 1"))
        .stdout(predicate::str::contains("\"a\""))
        .stdout(predicate::str::contains("\"b\""));
}

#[test]
fn cli_does_not_flag_a_simple_include_chain() {
    let dir = fresh_temp_dir("struct-cycle-report-chain");
    let shape_file = dir.join("shape.lisp");
    let line_file = dir.join("line.lisp");
    fs::write(&shape_file, "(defstruct shape (name nil))\n").expect("write shape.lisp");
    fs::write(
        &line_file,
        "(defstruct (line (:include shape)) (length 0))\n",
    )
    .expect("write line.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("struct-cycles")
        .arg("--output")
        .arg("json")
        .arg(&shape_file)
        .arg(&line_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"cycle_count\": 0"));
}

#[test]
fn cli_struct_cycles_fail_on_cycle_trips_gate() {
    let dir = fresh_temp_dir("struct-cycle-report-gate");
    let a_file = dir.join("a.lisp");
    let b_file = dir.join("b.lisp");
    fs::write(&a_file, "(defstruct (a (:include b)) (x 0))\n").expect("write a.lisp");
    fs::write(&b_file, "(defstruct (b (:include a)) (y 0))\n").expect("write b.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("struct-cycles")
        .arg("--fail-on-cycle")
        .arg(&a_file)
        .arg(&b_file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "struct-cycle-report policy failed",
        ));
}

#[test]
fn cli_struct_cycles_passes_gate_when_acyclic() {
    let dir = fresh_temp_dir("struct-cycle-report-gate-clean");
    let shape_file = dir.join("shape.lisp");
    fs::write(&shape_file, "(defstruct shape (name nil))\n").expect("write shape.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("struct-cycles")
        .arg("--fail-on-cycle")
        .arg("--output")
        .arg("json")
        .arg(&shape_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"cycle_count\": 0"));
}
