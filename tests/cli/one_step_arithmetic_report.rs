use super::*;

#[test]
fn cli_flags_plus_one() {
    let dir = fresh_temp_dir("one-step-arithmetic-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setf i (+ i 1))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("one-step-arithmetic")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"arithmetic_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"kind\": \"1+\""))
        .stdout(predicate::str::contains("\"shorthand\": \"1+\""));
}

#[test]
fn cli_does_not_flag_one_minus_x_float_or_non_one() {
    let dir = fresh_temp_dir("one-step-arithmetic-report-clean");
    let file = dir.join("a.lisp");
    // (- 1 x) has no shorthand; a float 1.0 coerces; a non-1 literal is unrelated.
    fs::write(&file, "(- 1 x)\n(+ x 1.0)\n(+ x 2)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("one-step-arithmetic")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "three +/- forms, none a unit
        // step" from "no arithmetic at all".
        .stdout(predicate::str::contains("\"arithmetic_form_count\": 3"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_one_step_arithmetic_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("one-step-arithmetic-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn bump [i] (+ i 1))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "one-step-arithmetic", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_one_step_arithmetic_emits_sarif() {
    let dir = fresh_temp_dir("one-step-arithmetic-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(- count 1)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "one-step-arithmetic", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/one-step-arithmetic/1-\"",
        ))
        .stdout(predicate::str::contains(
            "add/subtract of 1 has a shorthand; use 1-",
        ));
}

#[test]
fn cli_one_step_arithmetic_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("one-step-arithmetic-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(- count 1)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("one-step-arithmetic")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "one-step-arithmetic-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_uses_the_shorthand() {
    let dir = fresh_temp_dir("one-step-arithmetic-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(+ 1 (length xs))\n(- count 1)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("one-step-arithmetic")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(1+ (length xs))\n(1- count)\n");
}
