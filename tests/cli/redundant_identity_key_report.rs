use super::*;

#[test]
fn cli_flags_explicit_identity_key() {
    let dir = fresh_temp_dir("redundant-identity-key-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(sort xs #'< :key #'identity)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-identity-key")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"call_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"head\": \"sort\""));
}

#[test]
fn cli_flags_nil_key() {
    let dir = fresh_temp_dir("redundant-identity-key-report-nil");
    let file = dir.join("a.lisp");
    fs::write(&file, "(find x list :key nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-identity-key")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"));
}

#[test]
fn cli_does_not_flag_custom_key_or_non_key_head() {
    let dir = fresh_temp_dir("redundant-identity-key-report-clean");
    let file = dir.join("a.lisp");
    // A custom key, and tree-equal (which takes :test but no :key), are left alone.
    fs::write(
        &file,
        "(sort xs #'< :key #'car)\n(tree-equal a b :key #'identity)\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-identity-key")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no redundant :key in one
        // :key-taking call" from "no such call at all"; tree-equal is not one.
        .stdout(predicate::str::contains("\"call_form_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("redundant-identity-key-report-unmodelled");
    let file = dir.join("a.clj");
    fs::write(&file, "(sort xs #'< :key #'identity)\n").expect("write a.clj");

    paredit()
        .args(["inspect", "redundant-identity-key", "--output", "json"])
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
fn cli_redundant_identity_key_emits_sarif() {
    let dir = fresh_temp_dir("redundant-identity-key-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(sort xs #'< :key #'identity)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "redundant-identity-key", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/redundant-identity-key/redundant-identity-key\"",
        ))
        .stdout(predicate::str::contains(
            "sort defaults :key to identity; the explicit :key #'identity is redundant",
        ));
}

#[test]
fn cli_redundant_identity_key_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("redundant-identity-key-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(remove x seq :key #'identity)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-identity-key")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "redundant-identity-key-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_deletes_the_key_pair() {
    let dir = fresh_temp_dir("redundant-identity-key-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(remove x seq :key #'identity :from-end t)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("redundant-identity-key")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(remove x seq :from-end t)\n");
}
