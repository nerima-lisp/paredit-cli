use super::*;

#[test]
fn cli_flags_append_singleton() {
    let dir = fresh_temp_dir("append-list-to-cons-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(append (list x) rest)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("append-list-to-cons")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"append_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        // Both operand spans were in the old JSON and stay in it.
        .stdout(predicate::str::contains("\"element_span\""))
        .stdout(predicate::str::contains("\"rest_span\""));
}

#[test]
fn cli_does_not_flag_multi_element() {
    let dir = fresh_temp_dir("append-list-to-cons-report-multi");
    let file = dir.join("a.lisp");
    fs::write(&file, "(append (list x y) rest)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("append-list-to-cons")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "one append, not a singleton one"
        // from "no append at all".
        .stdout(predicate::str::contains("\"append_form_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_append_list_to_cons_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("append-list-to-cons-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(append (list x) rest)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("append-list-to-cons")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "append-list-to-cons-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_to_cons() {
    let dir = fresh_temp_dir("append-list-to-cons-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(append (list (car a)) (cdr b))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("append-list-to-cons")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(cons (car a) (cdr b))\n");
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model_append_list_to_cons() {
    let dir = fresh_temp_dir("append-list-to-cons-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn f [x r] (icollect [_ v (ipairs r)] v))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "append-list-to-cons", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

/// The envelope's interchange formats, which this report reached by moving onto
/// it. Asserted here only far enough to prove the command accepts them; their
/// content is covered once in `report_interop`.
#[test]
fn cli_append_list_to_cons_emits_sarif() {
    let dir = fresh_temp_dir("append-list-to-cons-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(append (list x) rest)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "append-list-to-cons", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/append-list-to-cons/append-list-to-cons\"",
        ))
        .stdout(predicate::str::contains(
            "a one-element append is just a cons; (append (list x) rest) is (cons x rest)",
        ));
}
