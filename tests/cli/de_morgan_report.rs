use super::*;

#[test]
fn cli_flags_and_of_nots() {
    let dir = fresh_temp_dir("de-morgan-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun neither? (a b) (and (not a) (not b)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("de-morgan")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"operator\": \"and\""));
}

#[test]
fn cli_does_not_flag_mixed_operands() {
    let dir = fresh_temp_dir("de-morgan-report-clean");
    let file = dir.join("a.lisp");
    // Mixed negated/plain, single negation, and a multi-arg not.
    fs::write(
        &file,
        "(and (not a) b)\n(and (not a))\n(or (not a b) (not c))\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("de-morgan")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_de_morgan_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("de-morgan-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(or (not ready) (not able))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("de-morgan")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("de-morgan-report policy failed"));
}

#[test]
fn cli_lint_fix_collapses_both_directions() {
    let dir = fresh_temp_dir("de-morgan-report-fix");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        "(and (not a) (not b))\n(or (not (foo x)) (not c))\n(and (not a) (not b) (not c))\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("de-morgan")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(
        fixed,
        "(not (or a b))\n(not (and (foo x) c))\n(not (or a b c))\n"
    );
}
