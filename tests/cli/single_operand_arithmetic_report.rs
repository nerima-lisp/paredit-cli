use super::*;

#[test]
fn cli_flags_single_operand_plus() {
    let dir = fresh_temp_dir("single-operand-arithmetic-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (x) (+ x))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("single-operand-arithmetic")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"arithmetic_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"operator\": \"+\""));
}

#[test]
fn cli_flags_single_operand_star() {
    let dir = fresh_temp_dir("single-operand-arithmetic-report-star");
    let file = dir.join("a.lisp");
    fs::write(&file, "(* n)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("single-operand-arithmetic")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"operator\": \"*\""));
}

#[test]
fn cli_does_not_flag_unary_minus_divide_or_multi_operand() {
    let dir = fresh_temp_dir("single-operand-arithmetic-report-clean");
    let file = dir.join("a.lisp");
    // (- x) negates, (/ x) is reciprocal, (+) is 0, (+ x y) is real arithmetic.
    fs::write(&file, "(- x)\n(/ x)\n(+)\n(+ x y)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("single-operand-arithmetic")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no redundant wrapper in two
        // scanned forms" from "no `+`/`*` form at all": `-` and `/` are not
        // this rule's operators, so only `(+)` and `(+ x y)` count.
        .stdout(predicate::str::contains("\"arithmetic_form_count\": 2"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("single-operand-arithmetic-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn f [x] (+ x))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "single-operand-arithmetic", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_single_operand_arithmetic_emits_sarif() {
    let dir = fresh_temp_dir("single-operand-arithmetic-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(* total)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "single-operand-arithmetic", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        // The operators are punctuation, so the rule's own name is the kind
        // and the operator rides along in the message.
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/single-operand-arithmetic/single-operand-arithmetic\"",
        ))
        .stdout(predicate::str::contains(
            "* has a single operand; (* X) is just X",
        ));
}

#[test]
fn cli_single_operand_arithmetic_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("single-operand-arithmetic-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(* total)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("single-operand-arithmetic")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "single-operand-arithmetic-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_unwraps_single_operand_arithmetic() {
    let dir = fresh_temp_dir("single-operand-arithmetic-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (x) (+ (compute x)))\n").expect("write a.lisp");

    // The aggregator's --fix engine should unwrap (+ (compute x)) to (compute x).
    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("single-operand-arithmetic")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(defun f (x) (compute x))\n");
}
