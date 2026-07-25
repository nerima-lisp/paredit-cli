use super::*;

#[test]
fn cli_reports_a_circular_depends_on_between_two_systems() {
    let dir = fresh_temp_dir("system-cycle-report");
    let app_file = dir.join("app.asd");
    let lib_file = dir.join("lib.asd");
    fs::write(
        &app_file,
        "(asdf:defsystem \"app\" :depends-on (\"lib\"))\n",
    )
    .expect("write app.asd");
    fs::write(
        &lib_file,
        "(asdf:defsystem \"lib\" :depends-on (\"app\"))\n",
    )
    .expect("write lib.asd");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("system-cycles")
        .arg("--output")
        .arg("json")
        .arg(&app_file)
        .arg(&lib_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"cycle_count\": 1"))
        .stdout(predicate::str::contains("\"app\""))
        .stdout(predicate::str::contains("\"lib\""));
}

#[test]
fn cli_does_not_flag_a_simple_depends_on_chain() {
    let dir = fresh_temp_dir("system-cycle-report-chain");
    let app_file = dir.join("app.asd");
    let util_file = dir.join("util.asd");
    fs::write(
        &app_file,
        "(asdf:defsystem \"app\" :depends-on (\"util\"))\n",
    )
    .expect("write app.asd");
    fs::write(&util_file, "(asdf:defsystem \"util\" :depends-on ())\n").expect("write util.asd");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("system-cycles")
        .arg("--output")
        .arg("json")
        .arg(&app_file)
        .arg(&util_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"cycle_count\": 0"));
}

#[test]
fn cli_system_cycles_fail_on_cycle_trips_gate() {
    let dir = fresh_temp_dir("system-cycle-report-gate");
    let app_file = dir.join("app.asd");
    let lib_file = dir.join("lib.asd");
    fs::write(
        &app_file,
        "(asdf:defsystem \"app\" :depends-on (\"lib\"))\n",
    )
    .expect("write app.asd");
    fs::write(
        &lib_file,
        "(asdf:defsystem \"lib\" :depends-on (\"app\"))\n",
    )
    .expect("write lib.asd");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("system-cycles")
        .arg("--fail-on-cycle")
        .arg(&app_file)
        .arg(&lib_file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "system-cycle-report policy failed",
        ));
}

#[test]
fn cli_system_cycles_passes_gate_when_acyclic() {
    let dir = fresh_temp_dir("system-cycle-report-gate-clean");
    let app_file = dir.join("app.asd");
    fs::write(&app_file, "(asdf:defsystem \"app\" :depends-on ())\n").expect("write app.asd");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("system-cycles")
        .arg("--fail-on-cycle")
        .arg("--output")
        .arg("json")
        .arg(&app_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"cycle_count\": 0"));
}
