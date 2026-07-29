use super::*;

#[test]
fn cli_flags_quoted_export() {
    let dir = fresh_temp_dir("defpackage-quoted-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defpackage :app (:export 'foo))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("defpackage-quoted")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"defpackage_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"clause\": \":export\""))
        // The clause leads every row, so a consumer can select one class of
        // mistake without parsing JSON.
        .stdout(predicate::str::contains("\"kind\": \":export\""))
        .stdout(predicate::str::contains("\"designator_span\""));
}

#[test]
fn cli_flags_multiple() {
    let dir = fresh_temp_dir("defpackage-quoted-report-multiple");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defpackage :app (:export 'foo 'bar))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("defpackage-quoted")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 2"));
}

#[test]
fn cli_does_not_flag_unquoted() {
    let dir = fresh_temp_dir("defpackage-quoted-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defpackage :app (:export foo) (:use :cl))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("defpackage-quoted")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no quoted designator in one
        // `defpackage`" from "no `defpackage` at all".
        .stdout(predicate::str::contains("\"defpackage_form_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("defpackage-quoted-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(defpackage :app (:export 'foo))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "defpackage-quoted", "--output", "json"])
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
fn cli_defpackage_quoted_emits_sarif() {
    let dir = fresh_temp_dir("defpackage-quoted-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defpackage :app (:use #'cl))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "defpackage-quoted", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/defpackage-quoted/:use\"",
        ))
        .stdout(predicate::str::contains(
            "defpackage does not evaluate its options; drop the quote in the :use clause",
        ));
}

#[test]
fn cli_defpackage_quoted_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("defpackage-quoted-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defpackage :app (:export 'foo))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("defpackage-quoted")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "defpackage-quoted-report policy failed",
        ));
}
