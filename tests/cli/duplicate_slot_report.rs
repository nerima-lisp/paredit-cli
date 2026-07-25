use super::*;

#[test]
fn cli_reports_a_defclass_with_a_duplicate_slot_name() {
    let dir = fresh_temp_dir("duplicate-slot-report");
    let file = dir.join("foo.lisp");
    fs::write(&file, "(defclass foo () (a a b))\n").expect("write foo.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-slots")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"duplicate_count\": 1"))
        .stdout(predicate::str::contains("\"slot\": \"a\""));
}

#[test]
fn cli_reports_a_defstruct_with_a_duplicate_slot_name() {
    let dir = fresh_temp_dir("duplicate-slot-report-struct");
    let file = dir.join("bar.lisp");
    fs::write(&file, "(defstruct bar (x 0) (x 1))\n").expect("write bar.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-slots")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"duplicate_count\": 1"))
        .stdout(predicate::str::contains("\"slot\": \"x\""));
}

#[test]
fn cli_does_not_flag_distinct_slot_names() {
    let dir = fresh_temp_dir("duplicate-slot-report-clean");
    let file = dir.join("foo.lisp");
    fs::write(&file, "(defclass foo () (a b c))\n").expect("write foo.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-slots")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"duplicate_count\": 0"));
}

#[test]
fn cli_duplicate_slots_fail_on_duplicate_trips_gate() {
    let dir = fresh_temp_dir("duplicate-slot-report-gate");
    let file = dir.join("foo.lisp");
    fs::write(&file, "(defclass foo () (a a))\n").expect("write foo.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-slots")
        .arg("--fail-on-duplicate")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "duplicate-slot-report policy failed",
        ));
}

#[test]
fn cli_duplicate_slots_passes_gate_when_all_slots_are_distinct() {
    let dir = fresh_temp_dir("duplicate-slot-report-gate-clean");
    let file = dir.join("foo.lisp");
    fs::write(&file, "(defclass foo () (a b))\n").expect("write foo.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-slots")
        .arg("--fail-on-duplicate")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"duplicate_count\": 0"));
}
