use super::*;

#[test]
fn cli_flags_two_form_prog2() {
    let dir = fresh_temp_dir("prog2-to-progn-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(prog2 (setup) (run))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("prog2-to-progn")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"prog2_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        // The operator token the fix rewrites, which the hand-written renderer
        // published and the envelope keeps.
        .stdout(predicate::str::contains("\"head_span\""));
}

#[test]
fn cli_does_not_flag_three_form() {
    let dir = fresh_temp_dir("prog2-to-progn-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(prog2 a b c)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("prog2-to-progn")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "the one prog2 here has three
        // forms" from "no prog2 at all".
        .stdout(predicate::str::contains("\"prog2_form_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_prog2_to_progn_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("prog2-to-progn-report-unmodelled");
    let file = dir.join("a.clj");
    fs::write(&file, "(prog2 a b)\n").expect("write a.clj");

    paredit()
        .args(["inspect", "prog2-to-progn", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_prog2_to_progn_emits_sarif() {
    let dir = fresh_temp_dir("prog2-to-progn-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(prog2 (setup) (run))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "prog2-to-progn", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/prog2-to-progn/prog2-to-progn\"",
        ))
        .stdout(predicate::str::contains(
            "a two-form prog2 is just progn; (prog2 a b) is (progn a b)",
        ));
}

#[test]
fn cli_prog2_to_progn_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("prog2-to-progn-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(prog2 a b)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("prog2-to-progn")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "prog2-to-progn-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_to_progn() {
    let dir = fresh_temp_dir("prog2-to-progn-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(prog2 (setup) (run))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("prog2-to-progn")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(progn (setup) (run))\n");
}
