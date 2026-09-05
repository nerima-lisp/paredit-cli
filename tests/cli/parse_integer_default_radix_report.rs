use super::*;

#[test]
fn cli_flags_radix_ten() {
    let dir = fresh_temp_dir("parse-integer-default-radix-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(parse-integer s :radix 10)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("parse-integer-default-radix")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"call_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"removal_span\""));
}

#[test]
fn cli_does_not_flag_non_ten() {
    let dir = fresh_temp_dir("parse-integer-default-radix-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(parse-integer s :radix 16)\n(parse-integer t)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("parse-integer-default-radix")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no restated default in two calls"
        // from "no parse-integer call at all".
        .stdout(predicate::str::contains("\"call_form_count\": 2"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_parse_integer_default_radix_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("parse-integer-default-radix-report-unmodelled");
    let file = dir.join("a.clj");
    fs::write(&file, "(parse-integer s :radix 10)\n").expect("write a.clj");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("parse-integer-default-radix")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_parse_integer_default_radix_emits_sarif() {
    let dir = fresh_temp_dir("parse-integer-default-radix-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(parse-integer s :radix 10)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("parse-integer-default-radix")
        .arg("--output")
        .arg("sarif")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/parse-integer-default-radix/parse-integer-default-radix\"",
        ))
        .stdout(predicate::str::contains(
            "explicit :radix 10 restates parse-integer's default; drop it",
        ));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("parse-integer-default-radix-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(parse-integer s :radix 10)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("parse-integer-default-radix")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "parse-integer-default-radix-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_drops_default_radix() {
    let dir = fresh_temp_dir("parse-integer-default-radix-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(parse-integer s :radix 10)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("parse-integer-default-radix")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(parse-integer s)\n");
}
