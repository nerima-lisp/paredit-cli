use super::*;

#[test]
fn cli_flags_typep_string() {
    let dir = fresh_temp_dir("typep-predicate-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(typep obj 'string)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("typep-predicate")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"typep_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"predicate\": \"stringp\""))
        // The object operand's span survived the move; the fix is
        // reconstructible from the report alone.
        .stdout(predicate::str::contains("\"object_span\""));
}

#[test]
fn cli_does_not_flag_unknown_type() {
    let dir = fresh_temp_dir("typep-predicate-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(typep x 'fixnum)\n(typep y 'standard-object)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("typep-predicate")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "neither typep has a dedicated
        // predicate" from "there is no typep at all".
        .stdout(predicate::str::contains("\"typep_form_count\": 2"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_typep_predicate_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("typep-predicate-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn check [x] (typep x 'string))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "typep-predicate", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_typep_predicate_emits_sarif() {
    let dir = fresh_temp_dir("typep-predicate-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(typep obj 'string)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "typep-predicate", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/typep-predicate/stringp\"",
        ))
        .stdout(predicate::str::contains(
            "typep against this type has a dedicated predicate; use (stringp x)",
        ));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("typep-predicate-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(typep x 'null)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("typep-predicate")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "typep-predicate-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_to_predicate() {
    let dir = fresh_temp_dir("typep-predicate-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(typep (car x) 'cons)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("typep-predicate")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(consp (car x))\n");
}
