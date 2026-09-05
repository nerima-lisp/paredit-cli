use super::*;

#[test]
fn cli_flags_negated_if() {
    let dir = fresh_temp_dir("negated-if-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun pick (ready) (if (not ready) 0 1))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("negated-if")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"if_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"kind\": \"negated-if\""));
}

#[test]
fn cli_does_not_flag_one_armed_or_positive_if() {
    let dir = fresh_temp_dir("negated-if-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if (not c) a)\n(if c a b)\n(if (not a b) x y)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("negated-if")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no negated test among three `if`
        // forms" from "no `if` form at all".
        .stdout(predicate::str::contains("\"if_form_count\": 3"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("negated-if-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn pick [ready] (if (not ready) 0 1))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "negated-if", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_negated_if_emits_sarif() {
    let dir = fresh_temp_dir("negated-if-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if (not ready) 0 1)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "negated-if", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/negated-if/negated-if\"",
        ))
        .stdout(predicate::str::contains(
            "if test is negated; (if (not c) a b) is (if c b a)",
        ));
}

#[test]
fn cli_negated_if_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("negated-if-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if (null xs) 0 (length xs))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("negated-if")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("negated-if-report policy failed"));
}

#[test]
fn cli_lint_fix_drops_negation_and_swaps_branches() {
    let dir = fresh_temp_dir("negated-if-report-fix");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        "(if (not ready) (do-a x) (do-b y))\n(if (null xs) 0 (length xs))\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("negated-if")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(
        fixed,
        "(if ready (do-b y) (do-a x))\n(if xs (length xs) 0)\n"
    );
}
