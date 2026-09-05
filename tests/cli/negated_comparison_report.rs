use super::*;

#[test]
fn cli_flags_not_equal() {
    let dir = fresh_temp_dir("negated-comparison-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun distinct? (a b) (not (= a b)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("negated-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"negation_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        // The complement is a field rather than the `kind`: `/=` is punctuation
        // and would make the leading column of every text row unreadable.
        .stdout(predicate::str::contains("\"kind\": \"negated-comparison\""))
        .stdout(predicate::str::contains("\"complement\": \"/=\""));
}

#[test]
fn cli_flags_null_of_comparison() {
    let dir = fresh_temp_dir("negated-comparison-report-null");
    let file = dir.join("a.lisp");
    fs::write(&file, "(null (>= p q))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("negated-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"complement\": \"<\""));
}

#[test]
fn cli_does_not_flag_three_arg_or_non_comparison() {
    let dir = fresh_temp_dir("negated-comparison-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(not (= a b c))\n(not (evenp x))\n(not flag)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("negated-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no complement among three
        // negations" from "no negation at all".
        .stdout(predicate::str::contains("\"negation_form_count\": 3"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("negated-comparison-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn distinct? [a b] (not (= a b)))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "negated-comparison", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_negated_comparison_emits_sarif() {
    let dir = fresh_temp_dir("negated-comparison-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(not (= a b))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "negated-comparison", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/negated-comparison/negated-comparison\"",
        ))
        .stdout(predicate::str::contains(
            "negated comparison has a complement operator; use /=",
        ));
}

#[test]
fn cli_negated_comparison_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("negated-comparison-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(not (> x y))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("negated-comparison")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "negated-comparison-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_every_complement() {
    let dir = fresh_temp_dir("negated-comparison-report-fix");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        "(not (= a b))\n(not (/= a b))\n(not (< a b))\n(not (> a b))\n(not (<= a b))\n(not (>= a b))\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("negated-comparison")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(
        fixed,
        "(/= a b)\n(= a b)\n(>= a b)\n(<= a b)\n(> a b)\n(< a b)\n"
    );
}
