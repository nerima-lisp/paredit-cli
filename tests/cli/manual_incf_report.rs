use super::*;

#[test]
fn cli_flags_manual_increment() {
    let dir = fresh_temp_dir("manual-incf-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun step () (setf counter (1+ counter)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("manual-incf")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"assignment_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"suggested\": \"incf\""));
}

#[test]
fn cli_flags_manual_decrement_with_delta() {
    let dir = fresh_temp_dir("manual-incf-report-decf");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setq i (- i step))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("manual-incf")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"suggested\": \"decf\""));
}

#[test]
fn cli_does_not_flag_compound_place_or_other_variable() {
    let dir = fresh_temp_dir("manual-incf-report-clean");
    let file = dir.join("a.lisp");
    // Compound place, a different variable, a non-commuting minus, and a multi-pair setf.
    fs::write(
        &file,
        "(setf (aref a i) (1+ (aref a i)))\n(setf x (1+ y))\n(setf i (- step i))\n(setf x (1+ x) y (1+ y))\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("manual-incf")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "none of these four assignments is
        // a hand-written increment" from "there is no assignment at all".
        .stdout(predicate::str::contains("\"assignment_form_count\": 4"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("manual-incf-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn bump [] (setf x (1+ x)))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "manual-incf", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

/// The envelope's interchange formats, which this report reached by moving onto
/// it. Asserted here only far enough to prove the command accepts them; their
/// content is covered once in `report_interop`. The rule id carries the
/// suggested macro, so an increment and a decrement are separable.
#[test]
fn cli_manual_incf_emits_sarif() {
    let dir = fresh_temp_dir("manual-incf-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setq n (1- n))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "manual-incf", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/manual-incf/decf\"",
        ))
        .stdout(predicate::str::contains(
            "setf manually adjusts a variable; use decf",
        ));
}

#[test]
fn cli_manual_incf_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("manual-incf-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setf total (+ total 1))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("manual-incf")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("manual-incf-report policy failed"));
}

#[test]
fn cli_lint_fix_rewrites_manual_incf_forms() {
    let dir = fresh_temp_dir("manual-incf-report-fix");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        "(setf a (1+ a))\n(setf b (+ b 3))\n(setf c (+ 2 c))\n(setf d (- d 1))\n(setq e (1- e))\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("manual-incf")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(
        fixed,
        "(incf a)\n(incf b 3)\n(incf c 2)\n(decf d 1)\n(decf e)\n"
    );
}
