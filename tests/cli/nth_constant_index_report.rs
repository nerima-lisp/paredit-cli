use super::*;

#[test]
fn cli_flags_nth_zero() {
    let dir = fresh_temp_dir("nth-constant-index-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun head-of (xs) (nth 0 xs))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nth-constant-index")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"nth_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"ordinal\": \"first\""))
        // The ordinal is also the finding's kind, so it leads every text row
        // and namespaces the SARIF rule id.
        .stdout(predicate::str::contains("\"kind\": \"first\""));
}

#[test]
fn cli_does_not_flag_large_or_variable_index() {
    let dir = fresh_temp_dir("nth-constant-index-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(nth 10 x)\n(nth i x)\n(elt x 0)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nth-constant-index")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no constant index in two `nth`
        // forms" from "no `nth` form at all"; `elt` is not an `nth`.
        .stdout(predicate::str::contains("\"nth_form_count\": 2"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("nth-constant-index-report-unmodelled");
    let file = dir.join("a.clj");
    fs::write(&file, "(nth 0 xs)\n").expect("write a.clj");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nth-constant-index")
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
fn cli_nth_constant_index_emits_sarif() {
    let dir = fresh_temp_dir("nth-constant-index-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(nth 2 row)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nth-constant-index")
        .arg("--output")
        .arg("sarif")
        .arg(&file)
        .assert()
        .success()
        // The ordinal is the kind, so each ordinal gets its own rule id.
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/nth-constant-index/third\"",
        ))
        .stdout(predicate::str::contains(
            "nth with a constant index; use (third …)",
        ));
}

#[test]
fn cli_nth_constant_index_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("nth-constant-index-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(nth 2 row)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nth-constant-index")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "nth-constant-index-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_nth_to_ordinal() {
    let dir = fresh_temp_dir("nth-constant-index-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(nth 0 (rest pairs))\n(nth 1 row)\n(nth 9 cols)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("nth-constant-index")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(first (rest pairs))\n(second row)\n(tenth cols)\n");
}
