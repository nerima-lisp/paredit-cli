use super::*;

#[test]
fn cli_flags_cons_onto_nil() {
    let dir = fresh_temp_dir("cons-to-list-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun wrap (x) (cons x nil))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("cons-to-list")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"cons_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"));
}

#[test]
fn cli_does_not_flag_cons_onto_variable_or_pair() {
    let dir = fresh_temp_dir("cons-to-list-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cons a xs)\n(cons a b)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("cons-to-list")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "two genuine conses" from "no cons
        // at all".
        .stdout(predicate::str::contains("\"cons_form_count\": 2"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_cons_to_list_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("cons-to-list-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cons item (list rest))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("cons-to-list")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "cons-to-list-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_cons_as_list() {
    let dir = fresh_temp_dir("cons-to-list-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cons (f x) nil)\n(cons a (list b c))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("cons-to-list")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(list (f x))\n(list a b c)\n");
}

#[test]
fn cli_lint_fix_collapses_a_cons_chain() {
    let dir = fresh_temp_dir("cons-to-list-report-fixpoint");
    let file = dir.join("a.lisp");
    // (cons a (cons b (cons c nil))) converges to (list a b c) one layer per pass.
    fs::write(&file, "(cons a (cons b (cons c nil)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("cons-to-list")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(list a b c)\n");
}

#[test]
fn cli_labels_a_dialect_the_rule_does_not_model_cons_to_list() {
    let dir = fresh_temp_dir("cons-to-list-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn wrap [x] [x])\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "cons-to-list", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_cons_to_list_emits_sarif() {
    let dir = fresh_temp_dir("cons-to-list-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun wrap (x) (cons x nil))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "cons-to-list", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/cons-to-list/cons-to-list\"",
        ))
        .stdout(predicate::str::contains(
            "cons onto nil/a list is a list constructor; use list",
        ));
}
