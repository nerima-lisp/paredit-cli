use super::*;

#[test]
fn cli_flags_nreverse_of_a_quoted_list() {
    let dir = fresh_temp_dir("destructive-literal-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f () (nreverse '(a b c)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("destructive-literal")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"destructive_call_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"operator\": \"nreverse\""))
        // The rendered literal was in the old JSON and stays in it.
        .stdout(predicate::str::contains("\"literal\""));
}

#[test]
fn cli_flags_sort_of_a_quoted_list() {
    let dir = fresh_temp_dir("destructive-literal-report-sort");
    let file = dir.join("a.lisp");
    fs::write(&file, "(sort '(3 1 2) #'<)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("destructive-literal")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"operator\": \"sort\""));
}

#[test]
fn cli_flags_later_argument_and_set_operation_sequences() {
    let dir = fresh_temp_dir("destructive-literal-report-positions");
    let file = dir.join("a.lisp");
    // delete: seq is arg 2; nsubstitute: arg 3; nunion: either list.
    // The literal ITEM of delete and a literal as the LAST nconc arg are safe.
    fs::write(
        &file,
        "(delete x '(1 2 3))\n(nsubstitute a b '(1 2))\n(nunion xs '(3 4))\n(delete '(k) xs)\n(nconc xs '(9))\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("destructive-literal")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        // delete (seq), nsubstitute (seq), nunion (list2) flag; the delete item
        // literal and the last-arg nconc literal do not.
        .stdout(predicate::str::contains("\"finding_count\": 3"));
}

#[test]
fn cli_does_not_flag_fresh_lists_or_variables() {
    let dir = fresh_temp_dir("destructive-literal-report-clean");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        "(nreverse (list a b))\n(sort xs #'<)\n(reverse '(1 2 3))\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("destructive-literal")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // Two destructive calls were scanned (`reverse` is not one); the
        // denominator says so rather than leaving it to be inferred.
        .stdout(predicate::str::contains("\"destructive_call_count\": 2"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_destructive_literal_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("destructive-literal-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(nbutlast '(1 2 3))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("destructive-literal")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "destructive-literal-report policy failed",
        ));
}

#[test]
fn cli_destructive_literal_expands_directory_inputs() {
    let dir = fresh_temp_dir("destructive-literal-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(defun g (x) (nconc x (nreverse '(1 2))))\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("destructive-literal")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model_destructive_literal() {
    let dir = fresh_temp_dir("destructive-literal-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn f [] (table.sort [3 1 2]))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "destructive-literal", "--output", "json"])
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
fn cli_destructive_literal_emits_sarif() {
    let dir = fresh_temp_dir("destructive-literal-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(nreverse '(a b c))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "destructive-literal", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/destructive-literal/destructive-literal\"",
        ))
        .stdout(predicate::str::contains(
            "nreverse destructively modifies quoted literal",
        ));
}
