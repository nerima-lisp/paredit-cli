use super::*;

#[test]
fn cli_reports_a_repeated_keyword() {
    let dir = fresh_temp_dir("duplicate-lambda-list-keyword-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (&optional x &optional y) x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-lambda-list-keyword")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"duplicate_count\": 1"))
        .stdout(predicate::str::contains("\"&optional\""));
}

#[test]
fn cli_does_not_flag_distinct_keywords() {
    let dir = fresh_temp_dir("duplicate-lambda-list-keyword-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (a &optional b &rest c &key d) a)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-lambda-list-keyword")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"duplicate_count\": 0"));
}

#[test]
fn cli_does_not_flag_a_nested_destructuring_keyword() {
    let dir = fresh_temp_dir("duplicate-lambda-list-keyword-report-nested");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defmacro m ((a &optional b) &optional c) a)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-lambda-list-keyword")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"duplicate_count\": 0"));
}

#[test]
fn cli_duplicate_lambda_list_keyword_fail_on_duplicate_trips_gate() {
    let dir = fresh_temp_dir("duplicate-lambda-list-keyword-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (a &key b &key c) a)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-lambda-list-keyword")
        .arg("--fail-on-duplicate")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "duplicate-lambda-list-keyword-report policy failed",
        ));
}

#[test]
fn cli_duplicate_lambda_list_keyword_expands_directory_inputs() {
    let dir = fresh_temp_dir("duplicate-lambda-list-keyword-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(
        &file,
        "(defmethod area ((s circle) &optional x &optional y) 1)\n",
    )
    .expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-lambda-list-keyword")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"duplicate_count\": 1"));
}
