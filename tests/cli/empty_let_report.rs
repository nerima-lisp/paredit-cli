use super::*;

#[test]
fn cli_flags_empty_let() {
    let dir = fresh_temp_dir("empty-let-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(let () (foo) (bar))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("empty-let")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"let_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"));
}

#[test]
fn cli_does_not_flag_bindings_declare_or_let_star() {
    let dir = fresh_temp_dir("empty-let-report-clean");
    let file = dir.join("a.lisp");
    // A real binding, a leading declare (invalid in progn), and let* are all left alone.
    fs::write(
        &file,
        "(let ((x 1)) x)\n(let () (declare (ignore y)) (z))\n(let* () (w))\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("empty-let")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no empty binding list in two `let`
        // forms" from "no `let` form at all". `let*` is not counted here.
        .stdout(predicate::str::contains("\"let_form_count\": 2"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_empty_let_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("empty-let-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(let [] (foo))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "empty-let", "--output", "json"])
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
fn cli_empty_let_emits_sarif() {
    let dir = fresh_temp_dir("empty-let-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(let () (foo) (bar))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "empty-let", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/empty-let/empty-let\"",
        ))
        .stdout(predicate::str::contains(
            "let with no bindings is just progn",
        ));
}

#[test]
fn cli_empty_let_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("empty-let-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(let nil (run))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("empty-let")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("empty-let-report policy failed"));
}

#[test]
fn cli_lint_fix_rewrites_as_progn() {
    let dir = fresh_temp_dir("empty-let-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(let () (step) (finish))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("empty-let")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(progn (step) (finish))\n");
}
