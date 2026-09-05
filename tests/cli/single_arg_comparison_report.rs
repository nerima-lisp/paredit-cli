use super::*;

#[test]
fn cli_flags_single_arg_less_than() {
    let dir = fresh_temp_dir("single-arg-comparison-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (x) (when (< x) (go)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("single-arg-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"comparison_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"operator\": \"<\""));
}

#[test]
fn cli_flags_single_arg_equal() {
    let dir = fresh_temp_dir("single-arg-comparison-report-eq");
    let file = dir.join("a.lisp");
    fs::write(&file, "(= (length xs))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("single-arg-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"operator\": \"=\""));
}

#[test]
fn cli_does_not_flag_two_argument_comparison() {
    let dir = fresh_temp_dir("single-arg-comparison-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(< x y)\n(<= a b c)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("single-arg-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "both scanned comparisons have
        // their operands" from "no comparison form at all".
        .stdout(predicate::str::contains("\"comparison_form_count\": 2"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_does_not_flag_equality_predicates() {
    let dir = fresh_temp_dir("single-arg-comparison-report-eql");
    let file = dir.join("a.lisp");
    // (eql x) is the equality-arity rule's territory, not this one.
    fs::write(&file, "(eql x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("single-arg-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // Not merely unflagged: never scanned, so the denominator is 0 too.
        .stdout(predicate::str::contains("\"comparison_form_count\": 0"));
}

#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("single-arg-comparison-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn f [x] (< x))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "single-arg-comparison", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_single_arg_comparison_emits_sarif() {
    let dir = fresh_temp_dir("single-arg-comparison-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(> n)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "single-arg-comparison", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        // The operators are punctuation, so the rule's own name is the kind
        // and the operator rides along in the message.
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/single-arg-comparison/single-arg-comparison\"",
        ))
        .stdout(predicate::str::contains(
            "> has a single argument; the comparison is always true (missing an operand?)",
        ));
}

#[test]
fn cli_single_arg_comparison_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("single-arg-comparison-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(> n)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("single-arg-comparison")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "single-arg-comparison-report policy failed",
        ));
}

#[test]
fn cli_single_arg_comparison_expands_directory_inputs() {
    let dir = fresh_temp_dir("single-arg-comparison-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(defun f (n) (/= n))\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("single-arg-comparison")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"));
}
