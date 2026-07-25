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
        .stdout(predicate::str::contains("\"violation_count\": 1"));
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
        .stdout(predicate::str::contains("\"violation_count\": 0"));
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
