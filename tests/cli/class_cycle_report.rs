use super::*;

#[test]
fn cli_reports_a_circular_inheritance_between_two_classes() {
    let dir = fresh_temp_dir("class-cycle-report");
    let a_file = dir.join("a.lisp");
    let b_file = dir.join("b.lisp");
    fs::write(&a_file, "(defclass a (b) ())\n").expect("write a.lisp");
    fs::write(&b_file, "(defclass b (a) ())\n").expect("write b.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("class-cycles")
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
fn cli_reports_a_cycle_mixing_defclass_and_define_condition() {
    let dir = fresh_temp_dir("class-cycle-report-condition");
    let a_file = dir.join("a.lisp");
    let b_file = dir.join("b.lisp");
    fs::write(&a_file, "(defclass app (app-error) ())\n").expect("write a.lisp");
    fs::write(&b_file, "(define-condition app-error (app) ())\n").expect("write b.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("class-cycles")
        .arg("--output")
        .arg("json")
        .arg(&a_file)
        .arg(&b_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"cycle_count\": 1"))
        .stdout(predicate::str::contains("\"app-error\""));
}

#[test]
fn cli_does_not_flag_a_simple_inheritance_chain() {
    let dir = fresh_temp_dir("class-cycle-report-chain");
    let base_file = dir.join("base.lisp");
    let derived_file = dir.join("derived.lisp");
    fs::write(&base_file, "(defclass base () ())\n").expect("write base.lisp");
    fs::write(&derived_file, "(defclass derived (base) ())\n").expect("write derived.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("class-cycles")
        .arg("--output")
        .arg("json")
        .arg(&base_file)
        .arg(&derived_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"cycle_count\": 0"));
}

#[test]
fn cli_class_cycles_fail_on_cycle_trips_gate() {
    let dir = fresh_temp_dir("class-cycle-report-gate");
    let a_file = dir.join("a.lisp");
    let b_file = dir.join("b.lisp");
    fs::write(&a_file, "(defclass a (b) ())\n").expect("write a.lisp");
    fs::write(&b_file, "(defclass b (a) ())\n").expect("write b.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("class-cycles")
        .arg("--fail-on-cycle")
        .arg(&a_file)
        .arg(&b_file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("class-cycle-report policy failed"));
}

#[test]
fn cli_class_cycles_passes_gate_when_acyclic() {
    let dir = fresh_temp_dir("class-cycle-report-gate-clean");
    let base_file = dir.join("base.lisp");
    fs::write(&base_file, "(defclass base () ())\n").expect("write base.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("class-cycles")
        .arg("--fail-on-cycle")
        .arg("--output")
        .arg("json")
        .arg(&base_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"cycle_count\": 0"));
}
