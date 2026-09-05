use super::*;

#[test]
fn cli_flags_repeated_setf_place() {
    let dir = fresh_temp_dir("duplicate-setf-place-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun reset () (setf total 0 total 1))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-setf-places")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"assignment_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"operator\": \"setf\""))
        .stdout(predicate::str::contains("\"place\": \"total\""));
}

#[test]
fn cli_does_not_flag_distinct_places_or_compound() {
    let dir = fresh_temp_dir("duplicate-setf-place-report-clean");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        "(setf a 1 b 2 c 3)\n(setf (aref v i) 1 (aref v i) 2)\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-setf-places")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no repeated place in two
        // assignment forms" from "no assignment form at all".
        .stdout(predicate::str::contains("\"assignment_form_count\": 2"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_duplicate_setf_place_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("duplicate-setf-place-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(set a 1)\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "duplicate-setf-places", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_duplicate_setf_place_emits_sarif() {
    let dir = fresh_temp_dir("duplicate-setf-place-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setf a 1 a 2)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "duplicate-setf-places", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/duplicate-setf-places/duplicate-setf-places\"",
        ))
        .stdout(predicate::str::contains(
            "setf assigns variable a more than once; the earlier assignment is dead",
        ));
}

#[test]
fn cli_duplicate_setf_place_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("duplicate-setf-place-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setq x 1 y 2 x 3)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-setf-places")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "duplicate-setf-place-report policy failed",
        ));
}

#[test]
fn cli_lint_marks_duplicate_setf_places_as_error_and_duplicate_category() {
    let dir = fresh_temp_dir("duplicate-setf-place-report-lint");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setf a 1 a 2)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("duplicate-setf-places")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"rule\": \"duplicate-setf-places\"",
        ))
        .stdout(predicate::str::contains("\"severity\": \"error\""))
        .stdout(predicate::str::contains("\"category\": \"duplicate\""))
        .stdout(predicate::str::contains("\"fixable\": false"));
}

#[test]
fn cli_duplicate_setf_place_ignored_by_fix_check() {
    let dir = fresh_temp_dir("duplicate-setf-place-report-fixcheck");
    let file = dir.join("a.lisp");
    // Report-only: --fix --check has nothing to apply for this rule.
    fs::write(&file, "(setf a 1 a 2)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("duplicate-setf-places")
        .arg("--fix")
        .arg("--check")
        .arg(&file)
        .assert()
        .success();
}
