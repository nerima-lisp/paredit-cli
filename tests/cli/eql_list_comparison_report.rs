use super::*;

#[test]
fn cli_reports_eql_against_a_quoted_list() {
    let dir = fresh_temp_dir("eql-list-comparison-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(when (eql p '(:a :b)) 1)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("eql-list-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"comparison_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"operator\": \"eql\""))
        .stdout(predicate::str::contains("(:a :b)"));
}

#[test]
fn cli_does_not_flag_a_quoted_symbol() {
    let dir = fresh_temp_dir("eql-list-comparison-report-symbol");
    let file = dir.join("a.lisp");
    fs::write(&file, "(eq x 'foo)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("eql-list-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no quoted list in one `eq`" from
        // "no `eq`/`eql` at all".
        .stdout(predicate::str::contains("\"comparison_form_count\": 1"));
}

#[test]
fn cli_does_not_flag_equal_against_a_quoted_list() {
    let dir = fresh_temp_dir("eql-list-comparison-report-equal");
    let file = dir.join("a.lisp");
    fs::write(&file, "(equal x '(1 2))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("eql-list-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        .stdout(predicate::str::contains("\"comparison_form_count\": 0"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("eql-list-comparison-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn pick [x] (= x [1 2]))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "eql-list-comparison", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_eql_list_comparison_emits_sarif() {
    let dir = fresh_temp_dir("eql-list-comparison-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(eq x '(1 2))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "eql-list-comparison", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        // The operator is the finding's kind, so `eq` and `eql` are separable
        // by rule id without parsing the JSON body.
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/eql-list-comparison/eq\"",
        ))
        .stdout(predicate::str::contains(
            "eq compares against quoted list literal (1 2)",
        ));
}

#[test]
fn cli_eql_list_comparison_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("eql-list-comparison-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(eq x '(1 2))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("eql-list-comparison")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "eql-list-comparison-report policy failed",
        ));
}

#[test]
fn cli_eql_list_comparison_passes_gate_when_clean() {
    let dir = fresh_temp_dir("eql-list-comparison-report-gate-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(eq x y)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("eql-list-comparison")
        .arg("--fail-on-violation")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}
