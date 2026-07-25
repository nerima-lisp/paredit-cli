use super::*;

#[test]
fn cli_flags_single_binding_let_star() {
    let dir = fresh_temp_dir("redundant-let-star-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(let* ((x 1)) (+ x x))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-let-star")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"binding_count\": 1"));
}

#[test]
fn cli_flags_zero_binding_let_star() {
    let dir = fresh_temp_dir("redundant-let-star-report-zero");
    let file = dir.join("a.lisp");
    fs::write(&file, "(let* () (side-effect))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-let-star")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"binding_count\": 0"));
}

#[test]
fn cli_does_not_flag_multi_binding_or_plain_let() {
    let dir = fresh_temp_dir("redundant-let-star-report-clean");
    let file = dir.join("a.lisp");
    // Two bindings genuinely use sequential scope; plain let is fine; a bare
    // `nil` binding list has no statically knowable count.
    fs::write(
        &file,
        "(let* ((x 1) (y (* x 2))) y)\n(let ((z 1)) z)\n(let* nil (foo))\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-let-star")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_redundant_let_star_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("redundant-let-star-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(let* ((only 1)) only)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-let-star")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "redundant-let-star-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_head_to_let() {
    let dir = fresh_temp_dir("redundant-let-star-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(let* ((x (compute))) (use x))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("redundant-let-star")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    // Only the head symbol changes; binding list and body stay byte-identical.
    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(let ((x (compute))) (use x))\n");
}
