use super::*;

#[test]
fn cli_flags_manual_push() {
    let dir = fresh_temp_dir("manual-push-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun record (item) (setf log (cons item log)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("manual-push")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"assignment_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"kind\": \"manual-push\""));
}

#[test]
fn cli_does_not_flag_cons_element_or_other_variable() {
    let dir = fresh_temp_dir("manual-push-report-clean");
    let file = dir.join("a.lisp");
    // Place consed as the element, cons onto another variable, compound place, multi-pair.
    fs::write(
        &file,
        "(setf xs (cons xs other))\n(setf xs (cons item ys))\n(setf (car n) (cons item (car n)))\n(setf xs (cons a xs) ys (cons b ys))\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("manual-push")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "none of these four assignments is
        // a hand-written push" from "there is no assignment at all".
        .stdout(predicate::str::contains("\"assignment_form_count\": 4"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("manual-push-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn record [item] (setf log (cons item log)))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "manual-push", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_manual_push_emits_sarif() {
    let dir = fresh_temp_dir("manual-push-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setf stack (cons top stack))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "manual-push", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/manual-push/manual-push\"",
        ))
        .stdout(predicate::str::contains(
            "setf conses onto a variable; use push",
        ));
}

#[test]
fn cli_manual_push_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("manual-push-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setf stack (cons top stack))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("manual-push")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("manual-push-report policy failed"));
}

#[test]
fn cli_lint_fix_rewrites_manual_push() {
    let dir = fresh_temp_dir("manual-push-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setf stack (cons (make-item) stack))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("manual-push")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(push (make-item) stack)\n");
}
