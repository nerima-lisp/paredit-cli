use super::*;

#[test]
fn cli_flags_apply_of_list_literal() {
    let dir = fresh_temp_dir("redundant-apply-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun run (a b) (apply #'process (list a b)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-apply")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"callee\": \"process\""))
        .stdout(predicate::str::contains("\"apply_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"));
}

#[test]
fn cli_does_not_flag_variable_list_or_intermediate_args() {
    let dir = fresh_temp_dir("redundant-apply-report-clean");
    let file = dir.join("a.lisp");
    // Variable list, leading fixed arg, ordinary quote, sharp-quoted lambda, funcall.
    fs::write(
        &file,
        "(apply #'foo args)\n(apply #'foo a (list b))\n(apply 'foo (list a))\n(apply #'(lambda (x) x) (list a))\n(funcall #'foo (list a))\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-apply")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no reducible shape among four
        // applies" from "no apply at all"; the trailing funcall is not one.
        .stdout(predicate::str::contains("\"apply_form_count\": 4"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_redundant_apply_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("redundant-apply-report-unmodelled");
    let file = dir.join("a.clj");
    fs::write(&file, "(apply #'foo (list a))\n").expect("write a.clj");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-apply")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_redundant_apply_emits_sarif() {
    let dir = fresh_temp_dir("redundant-apply-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(apply #'process (list a b))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-apply")
        .arg("--output")
        .arg("sarif")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/redundant-apply/redundant-apply\"",
        ))
        .stdout(predicate::str::contains(
            "apply of #'process to a literal list is a direct call",
        ));
}

#[test]
fn cli_redundant_apply_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("redundant-apply-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(apply #'handler (list event))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-apply")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "redundant-apply-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_apply_as_direct_call() {
    let dir = fresh_temp_dir("redundant-apply-report-fix");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        "(apply #'process (list (g x) 42))\n(apply #'reset (list))\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("redundant-apply")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(process (g x) 42)\n(reset)\n");
}
