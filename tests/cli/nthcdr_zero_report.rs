use super::*;

#[test]
fn cli_flags_nthcdr_zero() {
    let dir = fresh_temp_dir("nthcdr-zero-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(nthcdr 0 items)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nthcdr-zero")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"nthcdr_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"kind\": \"nthcdr-zero\""));
}

#[test]
fn cli_does_not_flag_nonzero_or_float() {
    let dir = fresh_temp_dir("nthcdr-zero-report-clean");
    let file = dir.join("a.lisp");
    // A non-0 index, a float 0.0, and a variable index are all left alone.
    fs::write(&file, "(nthcdr 1 x)\n(nthcdr 0.0 x)\n(nthcdr n x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nthcdr-zero")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no zero count in three nthcdr
        // calls" from "no nthcdr call at all".
        .stdout(predicate::str::contains("\"nthcdr_form_count\": 3"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("nthcdr-zero-report-unmodelled");
    let file = dir.join("a.clj");
    fs::write(&file, "(nthcdr 0 items)\n").expect("write a.clj");

    paredit()
        .args(["inspect", "nthcdr-zero", "--output", "json"])
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
fn cli_nthcdr_zero_emits_sarif() {
    let dir = fresh_temp_dir("nthcdr-zero-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(nthcdr 0 items)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "nthcdr-zero", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/nthcdr-zero/nthcdr-zero\"",
        ))
        .stdout(predicate::str::contains(
            "nthcdr with a zero count returns the list unchanged; (nthcdr 0 x) is x",
        ));
}

#[test]
fn cli_nthcdr_zero_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("nthcdr-zero-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(nthcdr 0 lst)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nthcdr-zero")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("nthcdr-zero-report policy failed"));
}

#[test]
fn cli_lint_fix_unwraps_to_the_list() {
    let dir = fresh_temp_dir("nthcdr-zero-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(nthcdr 0 (compute))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("nthcdr-zero")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(compute)\n");
}
