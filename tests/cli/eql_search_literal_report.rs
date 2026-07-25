use super::*;

#[test]
fn cli_flags_member_of_a_string_literal() {
    let dir = fresh_temp_dir("eql-search-literal-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (items) (member \"x\" items))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("eql-search-literal")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"operator\": \"member\""));
}

#[test]
fn cli_flags_assoc_of_a_quoted_list() {
    let dir = fresh_temp_dir("eql-search-literal-report-assoc");
    let file = dir.join("a.lisp");
    fs::write(&file, "(assoc '(a b) alist)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("eql-search-literal")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"operator\": \"assoc\""));
}

#[test]
fn cli_flags_substitute_old_value_and_adjoin() {
    let dir = fresh_temp_dir("eql-search-literal-report-positions");
    let file = dir.join("a.lisp");
    // substitute searches arg 2 (old); adjoin searches arg 1. The substitute
    // NEW value literal is not the searched item and must not flag.
    fs::write(
        &file,
        "(substitute new \"x\" seq)\n(adjoin \"y\" set)\n(substitute \"n\" old seq)\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("eql-search-literal")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 2"))
        .stdout(predicate::str::contains("\"operator\": \"substitute\""))
        .stdout(predicate::str::contains("\"operator\": \"adjoin\""));
}

#[test]
fn cli_does_not_flag_when_test_is_given_or_item_is_atomic() {
    let dir = fresh_temp_dir("eql-search-literal-report-clean");
    let file = dir.join("a.lisp");
    // Explicit :test, a number item, and a variable item are all fine.
    fs::write(
        &file,
        "(member \"x\" items :test #'equal)\n(find 5 xs)\n(position k xs)\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("eql-search-literal")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_eql_search_literal_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("eql-search-literal-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(find \"needle\" haystack)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("eql-search-literal")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "eql-search-literal-report policy failed",
        ));
}

#[test]
fn cli_eql_search_literal_expands_directory_inputs() {
    let dir = fresh_temp_dir("eql-search-literal-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(defun g (al) (rassoc \"v\" al))\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("eql-search-literal")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}
