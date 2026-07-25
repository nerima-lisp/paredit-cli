use super::*;

#[test]
fn cli_flags_cons_onto_nil() {
    let dir = fresh_temp_dir("cons-to-list-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun wrap (x) (cons x nil))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("cons-to-list")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_does_not_flag_cons_onto_variable_or_pair() {
    let dir = fresh_temp_dir("cons-to-list-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cons a xs)\n(cons a b)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("cons-to-list")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_cons_to_list_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("cons-to-list-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cons item (list rest))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("cons-to-list")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "cons-to-list-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_cons_as_list() {
    let dir = fresh_temp_dir("cons-to-list-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cons (f x) nil)\n(cons a (list b c))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("cons-to-list")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(list (f x))\n(list a b c)\n");
}

#[test]
fn cli_lint_fix_collapses_a_cons_chain() {
    let dir = fresh_temp_dir("cons-to-list-report-fixpoint");
    let file = dir.join("a.lisp");
    // (cons a (cons b (cons c nil))) converges to (list a b c) one layer per pass.
    fs::write(&file, "(cons a (cons b (cons c nil)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("cons-to-list")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(list a b c)\n");
}
