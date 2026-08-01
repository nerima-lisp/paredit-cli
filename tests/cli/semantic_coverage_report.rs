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
fn cli_reports_the_same_variable_bindings_for_a_defmethod_as_the_equivalent_defun() {
    // FR-009 golden fixture: a `defmethod`'s specialized required parameter
    // list must bind exactly the same names as a `defun`'s plain one —
    // `obj` and `arg`, not the specializer `my-type`.
    let dir = fresh_temp_dir("semantic-coverage-report-defmethod");
    let method = dir.join("method.lisp");
    let function = dir.join("function.lisp");
    fs::write(
        &method,
        "(defmethod handle ((obj my-type) arg) (list obj arg))\n",
    )
    .expect("write method.lisp");
    fs::write(&function, "(defun handle (obj arg) (list obj arg))\n").expect("write function.lisp");

    let mut method_cmd = paredit();
    method_cmd
        .arg("inspect")
        .arg("semantic-coverage")
        .arg("--output")
        .arg("json")
        .arg(&method)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"variable_bindings\": 2"));

    let mut function_cmd = paredit();
    function_cmd
        .arg("inspect")
        .arg("semantic-coverage")
        .arg("--output")
        .arg("json")
        .arg(&function)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"variable_bindings\": 2"));
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

#[test]
fn cli_fail_under_dialect_passes_when_that_dialects_resolution_clears_the_threshold() {
    let dir = fresh_temp_dir("semantic-coverage-report-dialect-gate-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(let ((x 1)) x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("semantic-coverage")
        .arg("--fail-under-dialect")
        .arg("common-lisp=90")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"dialect\": \"common-lisp\",\n      \"fail_under_dialect\": 90.0",
        ));
}

#[test]
fn cli_fail_under_dialect_trips_the_gate_when_that_dialects_resolution_is_too_low() {
    let dir = fresh_temp_dir("semantic-coverage-report-dialect-gate");
    let file = dir.join("a.lisp");
    // `(read)` is not a registered folding operator, so `x` never resolves.
    fs::write(&file, "(let ((x (read))) x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("semantic-coverage")
        .arg("--fail-under-dialect")
        .arg("common-lisp=50")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "inspect semantic-coverage policy failed",
        ))
        .stderr(predicate::str::contains("common-lisp"));
}

/// A dialect the corpus never discovered any files for fails loudly rather
/// than passing trivially, the same as an empty corpus does for
/// `--fail-under`.
#[test]
fn cli_fail_under_dialect_trips_the_gate_for_a_dialect_with_no_discovered_files() {
    let dir = fresh_temp_dir("semantic-coverage-report-dialect-gate-empty");
    let file = dir.join("a.lisp");
    fs::write(&file, "(let ((x 1)) x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("semantic-coverage")
        .arg("--fail-under-dialect")
        .arg("emacs-lisp=50")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("cannot evaluate an empty corpus"));
}

/// `--fail-under` and `--fail-under-dialect` combine with AND semantics: a
/// corpus-wide rate that clears `--fail-under` does not save the run when a
/// specific dialect's own rate still misses its `--fail-under-dialect`
/// floor.
#[test]
fn cli_fail_under_dialect_fails_the_run_even_when_the_global_threshold_passes() {
    let dir = fresh_temp_dir("semantic-coverage-report-dialect-gate-combined");
    let lisp_file = dir.join("a.lisp");
    let el_file = dir.join("a.el");
    fs::write(&lisp_file, "(let ((x 1)) x)\n").expect("write a.lisp");
    fs::write(&el_file, "(let ((x 1)) x)\n").expect("write a.el");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("semantic-coverage")
        .arg("--fail-under")
        .arg("50")
        .arg("--fail-under-dialect")
        .arg("emacs-lisp=50")
        .arg(&lisp_file)
        .arg(&el_file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("emacs-lisp"));
}

#[test]
fn cli_fail_under_dialect_rejects_a_malformed_value() {
    let dir = fresh_temp_dir("semantic-coverage-report-dialect-malformed");
    let file = dir.join("a.lisp");
    fs::write(&file, "(let ((x 1)) x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("semantic-coverage")
        .arg("--fail-under-dialect")
        .arg("not-a-dialect=90")
        .arg(&file)
        .assert()
        .code(2)
        .stderr(predicate::str::contains("--fail-under-dialect"));
}
