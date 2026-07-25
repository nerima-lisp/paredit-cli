use super::*;

#[test]
fn cli_reports_complexity_ranked_by_descending_score() {
    let dir = fresh_temp_dir("complexity-report");
    let lisp_file = dir.join("core.lisp");
    fs::write(
        &lisp_file,
        "(defun small (x) (+ x 1))\n\
         (defun big (x y z)\n\
           (if (> x y)\n\
               (let ((a (+ x y)))\n\
                 (if a (+ a z) z))\n\
               (- x y)))\n",
    )
    .expect("write lisp fixture");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("complexity")
        .arg("--output")
        .arg("json")
        .arg(&lisp_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"file_count\": 1"))
        .stdout(predicate::str::contains("\"definition_count\": 2"))
        .stdout(predicate::str::contains("\"name\": \"big\""))
        .stdout(predicate::str::contains("\"name\": \"small\""));
}

#[test]
fn cli_complexity_top_limits_ranked_leaderboard() {
    let dir = fresh_temp_dir("complexity-report-top");
    let lisp_file = dir.join("core.lisp");
    fs::write(
        &lisp_file,
        "(defun small (x) (+ x 1))\n\
         (defun big (x y z)\n\
           (if (> x y)\n\
               (let ((a (+ x y)))\n\
                 (if a (+ a z) z))\n\
               (- x y)))\n",
    )
    .expect("write lisp fixture");

    let mut cmd = paredit();
    let output = cmd
        .arg("inspect")
        .arg("complexity")
        .arg("--output")
        .arg("json")
        .arg("--top")
        .arg("1")
        .arg(&lisp_file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).expect("valid json");

    // --top limits only the cross-file leaderboard; the per-file inventory
    // stays complete so agents can still see every definition on request.
    let ranked = report["ranked"].as_array().expect("ranked array");
    assert_eq!(ranked.len(), 1);
    assert_eq!(ranked[0]["name"], "big");

    let definitions = report["files"][0]["definitions"]
        .as_array()
        .expect("definitions array");
    assert_eq!(definitions.len(), 2);
}

#[test]
fn cli_complexity_fail_on_max_depth_trips_gate() {
    let dir = fresh_temp_dir("complexity-report-gate");
    let lisp_file = dir.join("core.lisp");
    fs::write(
        &lisp_file,
        "(defun deep (x) (if x (if x (if x x x) x) x))\n",
    )
    .expect("write lisp fixture");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("complexity")
        .arg("--fail-on-max-depth")
        .arg("1")
        .arg(&lisp_file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("complexity-report policy failed"));
}

#[test]
fn cli_complexity_ignores_in_package_and_non_definition_forms() {
    let dir = fresh_temp_dir("complexity-report-ignores");
    let lisp_file = dir.join("core.lisp");
    fs::write(
        &lisp_file,
        "(in-package #:demo)\n(defun f (x) x)\n(+ 1 2)\n",
    )
    .expect("write lisp fixture");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("complexity")
        .arg("--output")
        .arg("json")
        .arg(&lisp_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"definition_count\": 1"));
}
