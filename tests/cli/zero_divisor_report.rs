use super::*;

#[test]
fn cli_flags_divide_by_zero() {
    let dir = fresh_temp_dir("zero-divisor-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(/ x 0)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("zero-divisor")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"division_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"operator\": \"/\""))
        // `divisor_span` points at the `0` itself, which the form's own span
        // only bounds; it survived the move onto the shared envelope.
        .stdout(predicate::str::contains("\"divisor_span\""));
}

#[test]
fn cli_flags_mod_zero() {
    let dir = fresh_temp_dir("zero-divisor-report-mod");
    let file = dir.join("a.lisp");
    fs::write(&file, "(mod x 0)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("zero-divisor")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"operator\": \"mod\""));
}

#[test]
fn cli_does_not_flag_zero_numerator() {
    let dir = fresh_temp_dir("zero-divisor-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(/ 0 x)\n(mod x 2)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("zero-divisor")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no zero divisor in two scanned
        // forms" from "no division form at all".
        .stdout(predicate::str::contains("\"division_form_count\": 2"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("zero-divisor-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn f [x] (/ x 0))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "zero-divisor", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_zero_divisor_emits_sarif() {
    let dir = fresh_temp_dir("zero-divisor-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(mod x 0)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "zero-divisor", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        // `/` is punctuation, so the rule's own name is the kind for every one
        // of the eleven heads, and the operator rides along in the message.
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/zero-divisor/zero-divisor\"",
        ))
        .stdout(predicate::str::contains(
            "mod by a literal 0 always signals division-by-zero",
        ));
}

#[test]
fn cli_zero_divisor_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("zero-divisor-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(/ x 0)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("zero-divisor")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "zero-divisor-report policy failed",
        ));
}
