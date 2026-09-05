use super::*;

#[test]
fn cli_flags_adjustable_nil() {
    let dir = fresh_temp_dir("make-array-default-keyword-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(make-array n :adjustable nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("make-array-default-keyword")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"call_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"keyword\": \":adjustable\""))
        // The fix's span, which the old report published and this one keeps.
        .stdout(predicate::str::contains("\"removal_span\""));
}

#[test]
fn cli_does_not_flag_non_nil() {
    let dir = fresh_temp_dir("make-array-default-keyword-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(make-array n :adjustable t)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("make-array-default-keyword")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no restated default in a
        // `make-array` call" from "no `make-array` call at all".
        .stdout(predicate::str::contains("\"call_form_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_make_array_default_keyword_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("make-array-default-keyword-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(make-array n :adjustable nil)\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "make-array-default-keyword", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_make_array_default_keyword_emits_sarif() {
    let dir = fresh_temp_dir("make-array-default-keyword-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(make-array n :fill-pointer nil)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "make-array-default-keyword", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/make-array-default-keyword/make-array-default-keyword\"",
        ))
        .stdout(predicate::str::contains(
            "explicit :fill-pointer nil restates make-array's default; drop it",
        ));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("make-array-default-keyword-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(make-array n :adjustable nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("make-array-default-keyword")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "make-array-default-keyword-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_drops_default_keyword() {
    let dir = fresh_temp_dir("make-array-default-keyword-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(make-array n :adjustable nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("make-array-default-keyword")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(make-array n)\n");
}
