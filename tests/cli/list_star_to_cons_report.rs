use super::*;

#[test]
fn cli_flags_two_arg_list_star() {
    let dir = fresh_temp_dir("list-star-to-cons-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(list* a b)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("list-star-to-cons")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"list_star_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        // Both operand spans the rewrite copies from, which the old report
        // published.
        .stdout(predicate::str::contains("\"car_span\""))
        .stdout(predicate::str::contains("\"cdr_span\""));
}

#[test]
fn cli_does_not_flag_three_args() {
    let dir = fresh_temp_dir("list-star-to-cons-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(list* a b c)\n(list* x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("list-star-to-cons")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no two-argument shape among two
        // `list*` forms" from "no `list*` form at all".
        .stdout(predicate::str::contains("\"list_star_form_count\": 2"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("list-star-to-cons-report-unmodelled");
    let file = dir.join("a.clj");
    fs::write(&file, "(list* a b)\n").expect("write a.clj");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("list-star-to-cons")
        .arg("--output")
        .arg("json")
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
fn cli_list_star_to_cons_emits_sarif() {
    let dir = fresh_temp_dir("list-star-to-cons-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(list* a b)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("list-star-to-cons")
        .arg("--output")
        .arg("sarif")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/list-star-to-cons/list-star-to-cons\"",
        ))
        .stdout(predicate::str::contains(
            "a two-argument list* is just a cons; (list* a b) is (cons a b)",
        ));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("list-star-to-cons-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(list* a b)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("list-star-to-cons")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "list-star-to-cons-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_to_cons() {
    let dir = fresh_temp_dir("list-star-to-cons-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(list* (car x) (cdr y))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("list-star-to-cons")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(cons (car x) (cdr y))\n");
}
