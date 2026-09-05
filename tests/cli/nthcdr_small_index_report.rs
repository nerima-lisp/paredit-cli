use super::*;

#[test]
fn cli_flags_nthcdr_one() {
    let dir = fresh_temp_dir("nthcdr-small-index-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(nthcdr 1 items)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nthcdr-small-index")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"nthcdr_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"accessor\": \"cdr\""))
        // The accessor is also the finding's kind, so it leads every text row
        // and namespaces the SARIF rule id.
        .stdout(predicate::str::contains("\"kind\": \"cdr\""))
        // The list operand's span is what a consumer needs to build the
        // (cdr x) rewrite itself, and the old report published it.
        .stdout(predicate::str::contains("\"list_span\""));
}

#[test]
fn cli_does_not_flag_zero_or_five() {
    let dir = fresh_temp_dir("nthcdr-small-index-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(nthcdr 0 x)\n(nthcdr 5 x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nthcdr-small-index")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no small count in two `nthcdr`
        // forms" from "no `nthcdr` form at all".
        .stdout(predicate::str::contains("\"nthcdr_form_count\": 2"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("nthcdr-small-index-report-unmodelled");
    let file = dir.join("a.clj");
    fs::write(&file, "(nthcdr 1 x)\n").expect("write a.clj");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nthcdr-small-index")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_nthcdr_small_index_emits_sarif() {
    let dir = fresh_temp_dir("nthcdr-small-index-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(nthcdr 2 x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nthcdr-small-index")
        .arg("--output")
        .arg("sarif")
        .arg(&file)
        .assert()
        .success()
        // The accessor is the kind, so each accessor gets its own rule id.
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/nthcdr-small-index/cddr\"",
        ))
        .stdout(predicate::str::contains(
            "nthcdr with a small count has a named cdr accessor; use (cddr …)",
        ));
}

#[test]
fn cli_nthcdr_small_index_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("nthcdr-small-index-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(nthcdr 2 x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nthcdr-small-index")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "nthcdr-small-index-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_to_accessor() {
    let dir = fresh_temp_dir("nthcdr-small-index-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(nthcdr 3 (rest ys))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("nthcdr-small-index")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(cdddr (rest ys))\n");
}
