use super::*;

#[test]
fn cli_flags_a_string_literal_destination() {
    let dir = fresh_temp_dir("format-missing-destination-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (x) (format \"~a~%\" x))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("format-missing-destination")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"format_call_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("~a~%"));
}

#[test]
fn cli_does_not_flag_nil_or_t_destinations() {
    let dir = fresh_temp_dir("format-missing-destination-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(format nil \"~a\" x)\n(format t \"~a\" y)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("format-missing-destination")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "two format calls, both with a
        // destination" from "no format call at all".
        .stdout(predicate::str::contains("\"format_call_count\": 2"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_does_not_flag_a_stream_destination() {
    let dir = fresh_temp_dir("format-missing-destination-report-stream");
    let file = dir.join("a.lisp");
    fs::write(&file, "(format out \"~a\" x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("format-missing-destination")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("format-missing-destination-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn f [x] (print x))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "format-missing-destination", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_format_missing_destination_emits_sarif() {
    let dir = fresh_temp_dir("format-missing-destination-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(format \"done\")\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "format-missing-destination", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/format-missing-destination/format-missing-destination\"",
        ))
        .stdout(predicate::str::contains(
            "format destination is the string literal \\\"done\\\"; a nil/t/stream destination is missing",
        ));
}

#[test]
fn cli_format_missing_destination_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("format-missing-destination-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(format \"done~%\")\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("format-missing-destination")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "format-missing-destination-report policy failed",
        ));
}

#[test]
fn cli_format_missing_destination_expands_directory_inputs() {
    let dir = fresh_temp_dir("format-missing-destination-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(defun f (x) (format \"~a\" x))\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("format-missing-destination")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"));
}
