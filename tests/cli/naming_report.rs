use super::*;

#[test]
fn cli_reports_non_idiomatic_names_across_definitions() {
    let dir = fresh_temp_dir("naming-report");
    let lisp_file = dir.join("core.lisp");
    fs::write(
        &lisp_file,
        "(defun render-pane (x) x)\n\
         (defun render_pane (x) x)\n\
         (defun renderPane (x) x)\n\
         (defvar *count* 0)\n",
    )
    .expect("write lisp fixture");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("naming")
        .arg("--output")
        .arg("json")
        .arg(&lisp_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"named_definition_count\": 4"))
        .stdout(predicate::str::contains("\"non_idiomatic_count\": 2"))
        .stdout(predicate::str::contains("\"name\": \"render_pane\""))
        .stdout(predicate::str::contains("\"style\": \"snake-case\""))
        .stdout(predicate::str::contains("\"name\": \"renderPane\""))
        .stdout(predicate::str::contains("\"style\": \"camel-case\""));
}

#[test]
fn cli_naming_fail_on_non_idiomatic_trips_gate() {
    let dir = fresh_temp_dir("naming-report-gate");
    let lisp_file = dir.join("core.lisp");
    fs::write(&lisp_file, "(defun render_pane (x) x)\n").expect("write lisp fixture");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("naming")
        .arg("--fail-on-non-idiomatic")
        .arg(&lisp_file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("naming-report policy failed"));
}

#[test]
fn cli_naming_passes_gate_for_idiomatic_names() {
    let dir = fresh_temp_dir("naming-report-clean");
    let lisp_file = dir.join("core.lisp");
    fs::write(
        &lisp_file,
        "(defun render-pane (x) x)\n(defvar *count* 0)\n(defconstant +limit+ 10)\n",
    )
    .expect("write lisp fixture");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("naming")
        .arg("--fail-on-non-idiomatic")
        .arg("--output")
        .arg("json")
        .arg(&lisp_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"non_idiomatic_count\": 0"));
}
