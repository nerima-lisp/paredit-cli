use super::*;

#[test]
fn cli_reports_json_totals_and_by_dialect() {
    let dir = fresh_temp_dir("semantic-coverage-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f ()\n  (let ((x 1))\n    (+ x 1)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("semantic-coverage")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"variable_bindings\": 1"))
        .stdout(predicate::str::contains("\"resolved_bindings\": 1"))
        .stdout(predicate::str::contains("\"dialect\": \"common-lisp\""));
}

#[test]
fn cli_reports_text_totals() {
    let dir = fresh_temp_dir("semantic-coverage-report-text");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f ()\n  (let ((x 1))\n    (+ x 1)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("semantic-coverage")
        .arg("--output")
        .arg("text")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("variable_bindings\t1/1"))
        .stdout(predicate::str::contains("common-lisp"));
}

#[test]
fn cli_expands_a_directory_argument() {
    let dir = fresh_temp_dir("semantic-coverage-report-dir");
    let sub = dir.join("sub");
    std::fs::create_dir_all(&sub).expect("create sub dir");
    fs::write(sub.join("x.lisp"), "(let ((x 1)) x)\n").expect("write x.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("semantic-coverage")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"file_count\": 1"));
}

#[test]
fn cli_dialect_override_measures_a_non_matching_extension_as_common_lisp() {
    let dir = fresh_temp_dir("semantic-coverage-report-dialect");
    let file = dir.join("a.txt");
    fs::write(&file, "(let ((x 1)) x)\n").expect("write a.txt");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("semantic-coverage")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect\": \"common-lisp\""));
}

#[test]
fn cli_fail_under_trips_the_gate_when_resolution_is_too_low() {
    let dir = fresh_temp_dir("semantic-coverage-report-gate");
    let file = dir.join("a.lisp");
    // `(read)` is not a registered folding operator, so `x` never resolves.
    fs::write(&file, "(let ((x (read))) x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("semantic-coverage")
        .arg("--fail-under")
        .arg("50")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "inspect semantic-coverage policy failed",
        ));
}

#[test]
fn cli_fail_under_passes_when_resolution_clears_the_threshold() {
    let dir = fresh_temp_dir("semantic-coverage-report-gate-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(let ((x 1)) x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("semantic-coverage")
        .arg("--fail-under")
        .arg("100")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"passed\": true"));
}

#[test]
fn cli_reports_a_decode_failure_without_failing_the_whole_run() {
    let dir = fresh_temp_dir("semantic-coverage-report-decode-error");
    let good = dir.join("good.lisp");
    let bad = dir.join("bad.lisp");
    fs::write(&good, "(let ((x 1)) x)\n").expect("write good.lisp");
    fs::write(&bad, [0x28, 0xff, 0xfe, 0x29]).expect("write invalid-utf8 bad.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("semantic-coverage")
        .arg("--output")
        .arg("json")
        .arg(&good)
        .arg(&bad)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"file_count\": 1"))
        .stdout(predicate::str::contains("\"stage\": \"decode\""));
}
