use super::*;

#[test]
fn cli_reports_two_files_declaring_the_same_system_name() {
    let dir = fresh_temp_dir("system-conflict-report");
    let a_file = dir.join("a.asd");
    let b_file = dir.join("b.asd");
    fs::write(&a_file, "(asdf:defsystem \"app\" :depends-on (\"lib\"))\n").expect("write a.asd");
    fs::write(&b_file, "(asdf:defsystem \"app\" :depends-on ())\n").expect("write b.asd");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("system-conflicts")
        .arg("--output")
        .arg("json")
        .arg(&a_file)
        .arg(&b_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"conflict_count\": 1"))
        .stdout(predicate::str::contains("\"app\""));
}

#[test]
fn cli_does_not_flag_distinct_system_names() {
    let dir = fresh_temp_dir("system-conflict-report-clean");
    let a_file = dir.join("a.asd");
    let b_file = dir.join("b.asd");
    fs::write(&a_file, "(asdf:defsystem \"app\" :depends-on ())\n").expect("write a.asd");
    fs::write(&b_file, "(asdf:defsystem \"lib\" :depends-on ())\n").expect("write b.asd");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("system-conflicts")
        .arg("--output")
        .arg("json")
        .arg(&a_file)
        .arg(&b_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"conflict_count\": 0"));
}

#[test]
fn cli_system_conflicts_fail_on_conflict_trips_gate() {
    let dir = fresh_temp_dir("system-conflict-report-gate");
    let a_file = dir.join("a.asd");
    let b_file = dir.join("b.asd");
    fs::write(&a_file, "(asdf:defsystem \"app\" :depends-on ())\n").expect("write a.asd");
    fs::write(&b_file, "(asdf:defsystem \"app\" :depends-on ())\n").expect("write b.asd");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("system-conflicts")
        .arg("--fail-on-conflict")
        .arg(&a_file)
        .arg(&b_file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "system-conflict-report policy failed",
        ));
}

#[test]
fn cli_system_conflicts_passes_gate_when_all_names_are_distinct() {
    let dir = fresh_temp_dir("system-conflict-report-gate-clean");
    let a_file = dir.join("a.asd");
    let b_file = dir.join("b.asd");
    fs::write(&a_file, "(asdf:defsystem \"app\" :depends-on ())\n").expect("write a.asd");
    fs::write(&b_file, "(asdf:defsystem \"lib\" :depends-on ())\n").expect("write b.asd");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("system-conflicts")
        .arg("--fail-on-conflict")
        .arg("--output")
        .arg("json")
        .arg(&a_file)
        .arg(&b_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"conflict_count\": 0"));
}
