use super::*;

#[test]
fn cli_reports_a_double_colon_reference_to_another_package() {
    let dir = fresh_temp_dir("package-boundary-report");
    let lisp_file = dir.join("core.lisp");
    fs::write(
        &lisp_file,
        "(in-package :app)\n(defun f () (other-pkg::internal-helper))\n",
    )
    .expect("write lisp fixture");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("package-boundaries")
        .arg("--output")
        .arg("json")
        .arg(&lisp_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains(
            "\"reference\": \"other-pkg::internal-helper\"",
        ))
        .stdout(predicate::str::contains(
            "\"target_package\": \"other-pkg\"",
        ))
        .stdout(predicate::str::contains("\"current_package\": \"app\""));
}

#[test]
fn cli_does_not_flag_single_colon_exported_access() {
    let dir = fresh_temp_dir("package-boundary-report-exported");
    let lisp_file = dir.join("core.lisp");
    fs::write(
        &lisp_file,
        "(in-package :app)\n(defun f () (other-pkg:public-api))\n",
    )
    .expect("write lisp fixture");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("package-boundaries")
        .arg("--output")
        .arg("json")
        .arg(&lisp_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_does_not_flag_a_double_colon_self_reference() {
    let dir = fresh_temp_dir("package-boundary-report-self");
    let lisp_file = dir.join("core.lisp");
    fs::write(
        &lisp_file,
        "(in-package :app)\n(defun f () (app::internal-helper))\n",
    )
    .expect("write lisp fixture");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("package-boundaries")
        .arg("--output")
        .arg("json")
        .arg(&lisp_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_package_boundaries_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("package-boundary-report-gate");
    let lisp_file = dir.join("core.lisp");
    fs::write(
        &lisp_file,
        "(in-package :app)\n(defun f () (other-pkg::internal-helper))\n",
    )
    .expect("write lisp fixture");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("package-boundaries")
        .arg("--fail-on-violation")
        .arg(&lisp_file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "package-boundary-report policy failed",
        ));
}

#[test]
fn cli_package_boundaries_passes_gate_for_exported_access_only() {
    let dir = fresh_temp_dir("package-boundary-report-gate-clean");
    let lisp_file = dir.join("core.lisp");
    fs::write(
        &lisp_file,
        "(in-package :app)\n(defun f () (other-pkg:public-api))\n",
    )
    .expect("write lisp fixture");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("package-boundaries")
        .arg("--fail-on-violation")
        .arg("--output")
        .arg("json")
        .arg(&lisp_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}
