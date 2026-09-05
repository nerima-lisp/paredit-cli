use super::*;

#[test]
fn cli_reports_incf_with_too_many_arguments() {
    let dir = fresh_temp_dir("modify-macro-arity-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(incf counter 1 2)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("modify-macro-arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"call_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"incf\""))
        .stdout(predicate::str::contains("\"argument_count\": 3"))
        .stdout(predicate::str::contains("\"min_arity\": 1"))
        .stdout(predicate::str::contains("\"max_arity\": 2"))
        .stdout(predicate::str::contains("\"1 or 2\""));
}

#[test]
fn cli_does_not_flag_valid_calls() {
    let dir = fresh_temp_dir("modify-macro-arity-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(incf x)\n(incf y 2)\n(push a stack)\n(pop stack)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("modify-macro-arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "four calls, all well-formed" from
        // "no modify-macro call at all".
        .stdout(predicate::str::contains("\"call_count\": 4"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_modify_macro_arity_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("modify-macro-arity-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn bump [x] (incf x 1 2))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "modify-macro-arity", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_modify_macro_arity_emits_sarif() {
    let dir = fresh_temp_dir("modify-macro-arity-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(pop)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "modify-macro-arity", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/modify-macro-arity/pop\"",
        ))
        .stdout(predicate::str::contains(
            "pop takes exactly 1 argument(s) but has 0",
        ));
}

#[test]
fn cli_does_not_flag_a_reader_conditional_argument() {
    let dir = fresh_temp_dir("modify-macro-arity-report-feature");
    let file = dir.join("a.lisp");
    fs::write(&file, "(decf z #+sbcl 1 #-sbcl 2)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("modify-macro-arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_flags_push_with_too_few_arguments() {
    let dir = fresh_temp_dir("modify-macro-arity-report-push");
    let file = dir.join("a.lisp");
    fs::write(&file, "(push item)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("modify-macro-arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"exactly 2\""));
}

#[test]
fn cli_modify_macro_arity_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("modify-macro-arity-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(pop)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("modify-macro-arity")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "modify-macro-arity-report policy failed",
        ));
}

#[test]
fn cli_modify_macro_arity_expands_directory_inputs() {
    let dir = fresh_temp_dir("modify-macro-arity-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(defun f (x) (incf x 1 2))\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("modify-macro-arity")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"));
}
