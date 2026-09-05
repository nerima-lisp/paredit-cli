use super::*;

#[test]
fn cli_flags_equals_zero() {
    let dir = fresh_temp_dir("sign-comparison-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun empty? (n) (= n 0))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("sign-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"comparison_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"predicate\": \"zerop\""));
}

#[test]
fn cli_flags_zero_on_left_flips_predicate() {
    let dir = fresh_temp_dir("sign-comparison-report-flip");
    let file = dir.join("a.lisp");
    fs::write(&file, "(> 0 balance)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("sign-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"predicate\": \"minusp\""));
}

#[test]
fn cli_does_not_flag_ge_le_float_or_nonzero() {
    let dir = fresh_temp_dir("sign-comparison-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(>= x 0)\n(<= x 0)\n(/= x 0)\n(= x 0.0)\n(= x 5)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("sign-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no comparison against 0 in two
        // scanned forms" from "no `=`/`>`/`<` form at all": `>=`, `<=` and `/=`
        // are not this rule's operators, so only the two `=` forms count.
        .stdout(predicate::str::contains("\"comparison_form_count\": 2"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("sign-comparison-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn empty? [n] (= n 0))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "sign-comparison", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_sign_comparison_emits_sarif() {
    let dir = fresh_temp_dir("sign-comparison-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(> count 0)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "sign-comparison", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/sign-comparison/plusp\"",
        ))
        .stdout(predicate::str::contains(
            "comparison against 0 has a dedicated predicate; use plusp",
        ));
}

#[test]
fn cli_sign_comparison_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("sign-comparison-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(< remaining 0)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("sign-comparison")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "sign-comparison-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_all_sign_predicates() {
    let dir = fresh_temp_dir("sign-comparison-report-fix");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        "(= (length xs) 0)\n(> count 0)\n(< delta 0)\n(< 0 amount)\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("sign-comparison")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(
        fixed,
        "(zerop (length xs))\n(plusp count)\n(minusp delta)\n(plusp amount)\n"
    );
}
