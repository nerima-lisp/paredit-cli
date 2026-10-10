use super::*;

#[test]
fn cli_extract_local_function_refuses_free_cell_mutation_without_writing() {
    let dir = fresh_temp_dir("extract-local-mutation");
    let file = dir.join("render.lisp");
    for selected in [
        "(push 1 x)",
        "(with-input-from-string (stream \"data\" :index x) (read stream))",
    ] {
        let input = format!("(defun render (x) (progn {selected} x))\n");
        fs::write(&file, &input).expect("write fixture");
        for write in [false, true] {
            let mut cmd = paredit();
            cmd.args([
                "refactor",
                "extract-local-function",
                "--path",
                "0.3.1",
                "--enclosing-path",
                "0.3",
                "--name",
                "compute",
                "--infer-params",
                "--output",
                "json",
            ])
            .arg("--file")
            .arg(&file);
            if write {
                cmd.arg("--write");
            }
            cmd.assert()
                .failure()
                .stderr(predicate::str::contains("assignment to free variable 'x'"));
            assert_eq!(
                fs::read_to_string(&file).expect("read unchanged fixture"),
                input
            );
        }
    }
}

#[test]
fn cli_extract_local_function_allows_local_cell_and_object_mutation() {
    for (selected, expected) in [
        (
            "(let ((x nil)) (push 1 x))",
            "(compute () (let ((x nil)) (push 1 x)))",
        ),
        ("(setf (car x) 1)", "(compute (x) (setf (car x) 1))"),
    ] {
        let mut cmd = paredit();
        cmd.args([
            "refactor",
            "extract-local-function",
            "--dialect",
            "common-lisp",
            "--path",
            "0.3.1",
            "--enclosing-path",
            "0.3",
            "--name",
            "compute",
            "--infer-params",
            "--output",
            "json",
        ])
        .write_stdin(format!("(defun render (x) (progn {selected} x))"))
        .assert()
        .success()
        .stdout(predicate::str::contains("\"written\": false"))
        .stdout(predicate::str::contains(expected));
    }
}

#[test]
fn cli_plans_extract_local_function_with_inferred_params() {
    let mut cmd = paredit();
    cmd.args([
        "refactor",
        "extract-local-function",
        "--dialect",
        "common-lisp",
        "--path",
        "0.3.1",
        "--enclosing-path",
        "0.3",
        "--name",
        "compute",
        "--infer-params",
        "--output",
        "json",
    ])
    .write_stdin("(defun render (x) (print (+ x 1)))")
    .assert()
    .success()
    .stdout(predicate::str::contains("\"params\": [\n    \"x\""))
    .stdout(predicate::str::contains("\"binding\": \"flet\""))
    .stdout(predicate::str::contains(
        "(defun render (x) (flet ((compute (x) (+ x 1))) (print (compute x))))",
    ));
}

#[test]
fn cli_uses_labels_for_recursive_local_function() {
    let mut cmd = paredit();
    cmd.args([
        "refactor",
        "extract-local-function",
        "--dialect",
        "common-lisp",
        "--path",
        "0.3.1",
        "--enclosing-path",
        "0.3",
        "--name",
        "compute",
        "--recursive",
        "--output",
        "json",
    ])
    .write_stdin("(defun render () (print (+ 1 2)))")
    .assert()
    .success()
    .stdout(predicate::str::contains("\"binding\": \"labels\""));
}

#[test]
fn cli_rejects_defmethod_specialized_lambda_list() {
    let mut cmd = paredit();
    cmd.args([
        "refactor",
        "extract-local-function",
        "--dialect",
        "common-lisp",
        "--path",
        "0.3",
        "--enclosing-path",
        "0",
        "--name",
        "compute",
        "--infer-params",
    ])
    .write_stdin("(defmethod render :around ((x t)) (print x))")
    .assert()
    .failure()
    .stderr(predicate::str::contains("structural binding position"));
}

#[test]
fn cli_accepts_standard_nil_alias_and_named_loop_targets() {
    for input in [
        "(progn (loop (return-from cl:nil 1)) (+ x 1))",
        "(progn (loop named done do (return-from done 1)) (+ x 1))",
    ] {
        let mut cmd = paredit();
        cmd.args([
            "refactor",
            "extract-local-function",
            "--dialect",
            "common-lisp",
            "--path",
            "0.1",
            "--enclosing-path",
            "0",
            "--name",
            "compute",
            "--output",
            "json",
        ])
        .write_stdin(input)
        .assert()
        .success();
    }
}

#[test]
fn cli_rejects_return_from_named_loop_without_nil_block() {
    let mut cmd = paredit();
    cmd.args([
        "refactor",
        "extract-local-function",
        "--dialect",
        "common-lisp",
        "--path",
        "0.2.1",
        "--enclosing-path",
        "0.2",
        "--name",
        "compute",
    ])
    .write_stdin("(block nil (progn (loop named done do (return 1)) (+ x 1)))")
    .assert()
    .failure()
    .stderr(predicate::str::contains("function boundary"));
}

#[test]
fn cli_accepts_runtime_structural_slots_and_integer_tag_aliases() {
    for (input, path) in [
        ("(let ((x (+ a b))) x)", "0.1.0.1"),
        (
            "(handler-bind ((error (function handle-error)) (warning #'handle-warning)) (work))",
            "0.1.0.1",
        ),
        (
            "(handler-bind ((error (function handle-error)) (warning #'handle-warning)) (work))",
            "0.1.1.1",
        ),
        ("(progn (tagbody #x10 (go 16)) (+ a b))", "0.1"),
    ] {
        let mut cmd = paredit();
        cmd.args([
            "refactor",
            "extract-local-function",
            "--dialect",
            "common-lisp",
            "--path",
            path,
            "--enclosing-path",
            "0",
            "--name",
            "compute",
            "--output",
            "json",
        ])
        .write_stdin(input)
        .assert()
        .success();
    }
}

#[test]
fn cli_accepts_restart_option_value_forms() {
    for (input, path) in [
        (
            "(restart-case (work) (retry () :interactive (+ a b) :report (+ c d) (declare (ignorable marker)) (+ e f)))",
            "0.2.3",
        ),
        (
            "(restart-case (work) (retry () :interactive (+ a b) :report (+ c d) (declare (ignorable marker)) (+ e f)))",
            "0.2.7",
        ),
        (
            "(restart-bind ((retry (+ a b) :interactive-function (+ c d) :report-function (+ e f))) (work))",
            "0.1.0.3",
        ),
        (
            "(restart-bind ((retry (+ a b) :interactive-function (+ c d) :report-function (+ e f))) (work))",
            "0.1.0.5",
        ),
    ] {
        let mut cmd = paredit();
        cmd.args([
            "refactor",
            "extract-local-function",
            "--dialect",
            "common-lisp",
            "--path",
            path,
            "--enclosing-path",
            "0",
            "--name",
            "compute",
            "--output",
            "json",
        ])
        .write_stdin(input)
        .assert()
        .success();
    }
}

#[test]
fn cli_rejects_structural_and_malformed_restart_options() {
    for (input, path) in [
        (
            "(restart-case (work) (retry () :interactive (+ a b) (+ c d)))",
            "0.2.2",
        ),
        (
            "(restart-case (work) (retry () :unknown (+ a b) (+ c d)))",
            "0.2.3",
        ),
        (
            "(restart-case (work) (retry () :interactive (+ a b) :test))",
            "0.2.3",
        ),
        (
            "(restart-bind ((retry (+ a b) :test-function (+ c d))) (work))",
            "0.1.0.2",
        ),
        (
            "(restart-bind ((retry (+ a b) :unknown (+ c d))) (work))",
            "0.1.0.3",
        ),
        (
            "(handler-bind ((error (+ a b) :test-function (+ c d))) (work))",
            "0.1.0.3",
        ),
    ] {
        let mut cmd = paredit();
        cmd.args([
            "refactor",
            "extract-local-function",
            "--dialect",
            "common-lisp",
            "--path",
            path,
            "--enclosing-path",
            "0",
            "--name",
            "compute",
        ])
        .write_stdin(input)
        .assert()
        .failure()
        .stderr(predicate::str::contains("structural binding position"));
    }
}

#[test]
fn cli_rejects_structural_condition_binding_type() {
    let mut cmd = paredit();
    cmd.args([
        "refactor",
        "extract-local-function",
        "--dialect",
        "common-lisp",
        "--path",
        "0.1.0.0",
        "--enclosing-path",
        "0",
        "--name",
        "compute",
    ])
    .write_stdin("(handler-bind ((error (function handle-error))) (work))")
    .assert()
    .failure()
    .stderr(predicate::str::contains("structural binding position"));
}

#[test]
fn cli_accepts_whole_executable_condition_forms() {
    for selected in [
        "(handler-case (work) (error (condition) (recover condition)))",
        "(restart-case (work) (retry () (work)))",
        "(handler-bind ((error #'handle-error)) (work))",
        "(restart-bind ((retry #'retry-work)) (work))",
        "(typecase value (integer (work)))",
        "(etypecase value (integer (work)))",
        "(ctypecase value (integer (work)))",
        "(case value (1 (work)))",
        "(ccase value (1 (work)))",
        "(ecase value (1 (work)))",
        "(eval-when (:execute) (work))",
        "(load-time-value (work) t)",
    ] {
        let input = format!("(progn {selected} (finish))");
        let mut cmd = paredit();
        cmd.args([
            "refactor",
            "extract-local-function",
            "--dialect",
            "common-lisp",
            "--path",
            "0.1",
            "--enclosing-path",
            "0",
            "--name",
            "compute",
            "--output",
            "json",
        ])
        .write_stdin(input)
        .assert()
        .success();
    }
}

#[test]
fn cli_rejects_non_evaluated_control_slots() {
    for (input, path) in [
        ("(case value ((1 2) (work)))", "0.2.0"),
        ("(eval-when (:compile-toplevel :execute) (work))", "0.1"),
        ("(load-time-value (work) t)", "0.2"),
    ] {
        let mut cmd = paredit();
        cmd.args([
            "refactor",
            "extract-local-function",
            "--dialect",
            "common-lisp",
            "--path",
            path,
            "--enclosing-path",
            "0",
            "--name",
            "compute",
        ])
        .write_stdin(input)
        .assert()
        .failure()
        .stderr(predicate::str::contains("structural binding position"));
    }
}
