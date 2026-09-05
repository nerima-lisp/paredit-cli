use super::*;

#[test]
fn cli_reports_eql_against_a_string_literal() {
    let dir = fresh_temp_dir("eql-string-comparison-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(when (eql name \"root\") 1)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("eql-string-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"comparison_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"operator\": \"eql\""));
}

#[test]
fn cli_does_not_flag_a_character_literal_comparison() {
    let dir = fresh_temp_dir("eql-string-comparison-report-char");
    let file = dir.join("a.lisp");
    fs::write(&file, "(eql c #\\a)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("eql-string-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no string in one `eql`" from "no
        // `eq`/`eql` at all".
        .stdout(predicate::str::contains("\"comparison_form_count\": 1"));
}

#[test]
fn cli_does_not_flag_equal_against_a_string() {
    let dir = fresh_temp_dir("eql-string-comparison-report-equal");
    let file = dir.join("a.lisp");
    fs::write(&file, "(equal s \"root\")\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("eql-string-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("eql-string-comparison-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn root? [name] (= name \"root\"))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "eql-string-comparison", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_eql_string_comparison_emits_sarif() {
    let dir = fresh_temp_dir("eql-string-comparison-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(eql name \"root\")\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "eql-string-comparison", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        // The operator is the finding's kind, so `eq` and `eql` are separable
        // by rule id without parsing the JSON body.
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/eql-string-comparison/eql\"",
        ))
        .stdout(predicate::str::contains("compares against string literal"));
}

#[test]
fn cli_eql_string_comparison_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("eql-string-comparison-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(eq x \"y\")\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("eql-string-comparison")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "eql-string-comparison-report policy failed",
        ));
}

#[test]
fn cli_eql_string_comparison_passes_gate_when_clean() {
    let dir = fresh_temp_dir("eql-string-comparison-report-gate-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(eql x y)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("eql-string-comparison")
        .arg("--fail-on-violation")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}
