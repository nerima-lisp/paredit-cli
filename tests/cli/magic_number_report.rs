use super::*;

#[test]
fn cli_flags_an_unnamed_numeric_literal_in_a_function_body() {
    let dir = fresh_temp_dir("magic-number-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (x) (+ x 42))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("magic-numbers")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"literal\": \"42\""))
        .stdout(predicate::str::contains("\"enclosing_definition\": \"f\""))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_does_not_flag_idiomatic_numbers() {
    let dir = fresh_temp_dir("magic-number-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (x) (if (= x -1) 0 (* x 2)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("magic-numbers")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

/// An empty finding list is ambiguous, so a dialect the value layer does not
/// model (Clojure is not Common Lisp or Emacs Lisp) must be labelled rather
/// than silently reported as clean.
#[test]
fn cli_magic_numbers_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("magic-number-report-unmodelled");
    let file = dir.join("a.clj");
    fs::write(&file, "(defn f [x] (+ x 42))\n").expect("write a.clj");

    paredit()
        .args(["inspect", "magic-numbers", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_magic_numbers_fail_on_magic_number_trips_gate() {
    let dir = fresh_temp_dir("magic-number-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (x) (+ x 42))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("magic-numbers")
        .arg("--fail-on-magic-number")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "inspect magic-numbers policy failed",
        ));
}

#[test]
fn cli_magic_numbers_passes_gate_on_a_clean_file() {
    let dir = fresh_temp_dir("magic-number-report-gate-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (x) (* x 2))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("magic-numbers")
        .arg("--fail-on-magic-number")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_magic_numbers_models_emacs_lisp_too() {
    let dir = fresh_temp_dir("magic-number-report-elisp");
    let file = dir.join("a.el");
    fs::write(&file, "(defun f (x) (+ x 42))\n").expect("write a.el");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("magic-numbers")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": true"))
        .stdout(predicate::str::contains("\"finding_count\": 1"));
}
