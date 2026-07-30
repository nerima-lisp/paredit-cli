use super::*;

#[test]
fn cli_flags_nested_car_cdr() {
    let dir = fresh_temp_dir("nested-cxr-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun second-of (pair) (car (cdr pair)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nested-cxr")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"combined\": \"cadr\""))
        // Both the outer `car` and the inner `cdr` are cXr forms scanned.
        .stdout(predicate::str::contains("\"accessor_form_count\": 2"))
        .stdout(predicate::str::contains("\"line\": 1"));
}

#[test]
fn cli_does_not_flag_single_accessor_or_non_accessor() {
    let dir = fresh_temp_dir("nested-cxr-report-clean");
    let file = dir.join("a.lisp");
    // Single accessor, car of a non-accessor, first/rest spelling, over-long combo.
    fs::write(
        &file,
        "(car x)\n(car (reverse x))\n(first (rest x))\n(caddr (caddr x))\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nested-cxr")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no combinable nesting among four
        // accessors" from "no accessor at all".
        .stdout(predicate::str::contains("\"accessor_form_count\": 4"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_nested_cxr_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("nested-cxr-report-unmodelled");
    let file = dir.join("a.clj");
    fs::write(&file, "(car (cdr x))\n").expect("write a.clj");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nested-cxr")
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
fn cli_nested_cxr_emits_sarif() {
    let dir = fresh_temp_dir("nested-cxr-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(car (cdr pair))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nested-cxr")
        .arg("--output")
        .arg("sarif")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/nested-cxr/nested-cxr\"",
        ))
        .stdout(predicate::str::contains(
            "nested car/cdr accessors combine into (cadr",
        ));
}

#[test]
fn cli_nested_cxr_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("nested-cxr-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cdr (cdr items))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nested-cxr")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("nested-cxr-report policy failed"));
}

#[test]
fn cli_lint_fix_collapses_one_level() {
    let dir = fresh_temp_dir("nested-cxr-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(car (cdr (lookup k)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("nested-cxr")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(cadr (lookup k))\n");
}

#[test]
fn cli_lint_fix_converges_deep_nesting_to_caddr() {
    let dir = fresh_temp_dir("nested-cxr-report-fixpoint");
    let file = dir.join("a.lisp");
    fs::write(&file, "(car (cdr (cdr x)))\n").expect("write a.lisp");

    // The fixpoint loop peels one accessor per pass: cadr, then caddr.
    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("nested-cxr")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(caddr x)\n");
}
