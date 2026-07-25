use super::*;

#[test]
fn cli_requires_file_for_inline_let_writes() {
    let mut cmd = paredit();
    cmd.args(["refactor", "inline-let", "--path", "0", "--write"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--write requires --file"));
}

#[test]
fn cli_plans_inline_let_for_common_lisp() {
    let mut cmd = paredit();
    cmd.args([
        "refactor",
        "inline-let",
        "--path",
        "0.3",
        "--dialect",
        "common-lisp",
        "--output",
        "json",
    ])
    .write_stdin("(defun render () (let ((product (* width height))) (+ product margin)))")
    .assert()
    .success()
    .stdout(predicate::str::contains("\"dialect\": \"common-lisp\""))
    .stdout(predicate::str::contains("\"binding_name\": \"product\""))
    .stdout(predicate::str::contains(
        "\"binding_value\": \"(* width height)\"",
    ))
    .stdout(predicate::str::contains("\"reference_count\": 1"))
    .stdout(predicate::str::contains(
        "(defun render () (+ (* width height) margin))",
    ));
}

#[test]
fn cli_plans_inline_let_for_common_lisp_symbol_macrolet() {
    let mut cmd = paredit();
    cmd.args([
        "refactor",
        "inline-let",
        "--path",
        "0.3",
        "--dialect",
        "common-lisp",
        "--output",
        "json",
    ])
    .write_stdin(
        "(defun render () (symbol-macrolet ((product (* width height))) (+ product margin)))",
    )
    .assert()
    .success()
    .stdout(predicate::str::contains("\"dialect\": \"common-lisp\""))
    .stdout(predicate::str::contains("\"binding_name\": \"product\""))
    .stdout(predicate::str::contains(
        "\"binding_value\": \"(* width height)\"",
    ))
    .stdout(predicate::str::contains("\"reference_count\": 1"))
    .stdout(predicate::str::contains(
        "(defun render () (+ (* width height) margin))",
    ));
}

#[test]
fn cli_plans_inline_let_with_multiple_body_expressions() {
    let mut cmd = paredit();
    cmd.args([
        "refactor",
        "inline-let",
        "--path",
        "0.3",
        "--allow-duplicate-evaluation",
        "--dialect",
        "common-lisp",
        "--output",
        "json",
    ])
    .write_stdin(
        "(defun render () (let ((product (* width height))) (log product) (+ product margin)))",
    )
    .assert()
    .success()
    .stdout(predicate::str::contains("\"body_count\": 2"))
    .stdout(predicate::str::contains("\"reference_count\": 2"))
    .stdout(predicate::str::contains(
        "(defun render () (log (* width height)) (+ (* width height) margin))",
    ));
}

#[test]
fn cli_writes_inline_let_for_emacs_lisp_file() {
    let dir = fresh_temp_dir("inline-let");
    let elisp_file = dir.join("render.el");
    fs::write(
        &elisp_file,
        "(defun render () (let ((product (* width height))) (+ product margin)))\n",
    )
    .expect("write elisp fixture");

    let mut cmd = paredit();
    cmd.arg("refactor")
        .arg("inline-let")
        .arg("--file")
        .arg(&elisp_file)
        .arg("--path")
        .arg("0.3")
        .arg("--write")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect\": \"emacs-lisp\""))
        .stdout(predicate::str::contains("\"written\": true"));

    assert_eq!(
        fs::read_to_string(elisp_file).expect("read inline let elisp"),
        "(defun render () (+ (* width height) margin))\n"
    );
}

#[test]
fn cli_writes_inline_let_for_emacs_lisp_cl_symbol_macrolet_file() {
    let dir = fresh_temp_dir("inline-let");
    let elisp_file = dir.join("render.el");
    fs::write(
        &elisp_file,
        "(defun render () (cl-symbol-macrolet ((product (* width height))) (+ product margin)))\n",
    )
    .expect("write elisp fixture");

    let mut cmd = paredit();
    cmd.arg("refactor")
        .arg("inline-let")
        .arg("--file")
        .arg(&elisp_file)
        .arg("--path")
        .arg("0.3")
        .arg("--write")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect\": \"emacs-lisp\""))
        .stdout(predicate::str::contains("\"written\": true"));

    assert_eq!(
        fs::read_to_string(elisp_file).expect("read inline let elisp"),
        "(defun render () (+ (* width height) margin))\n"
    );
}

#[test]
fn cli_rejects_inline_let_duplicate_evaluation_by_default() {
    let mut cmd = paredit();
    cmd.args([
        "refactor",
        "inline-let",
        "--path",
        "0",
        "--dialect",
        "common-lisp",
    ])
    .write_stdin("(let ((x (compute))) (+ x x))")
    .assert()
    .failure()
    .stderr(predicate::str::contains("would duplicate"));
}

#[test]
fn cli_allows_inline_let_duplicate_evaluation_when_explicit() {
    let mut cmd = paredit();
    cmd.args([
        "refactor",
        "inline-let",
        "--path",
        "0",
        "--allow-duplicate-evaluation",
        "--dialect",
        "common-lisp",
        "--output",
        "json",
    ])
    .write_stdin("(let ((x (compute))) (+ x x))")
    .assert()
    .success()
    .stdout(predicate::str::contains("\"reference_count\": 2"))
    .stdout(predicate::str::contains("(+ (compute) (compute))"));
}

#[test]
fn cli_plans_inline_let_for_all_known_stdin_dialects() {
    for (dialect, input) in [
        (
            "common-lisp",
            "(let ((product (* width height))) (+ product margin))",
        ),
        (
            "emacs-lisp",
            "(let ((product (* width height))) (+ product margin))",
        ),
        (
            "scheme",
            "(let ((product (* width height))) (+ product margin))",
        ),
        (
            "lfe",
            "(let ((product (* width height))) (+ product margin))",
        ),
        (
            "racket",
            "(let ((product (* width height))) (+ product margin))",
        ),
        (
            "clojure",
            "(let [product (* width height)] (+ product margin))",
        ),
        ("hy", "(let [product (* width height)] (+ product margin))"),
        (
            "carp",
            "(let [product (* width height)] (+ product margin))",
        ),
        (
            "janet",
            "(let [product (* width height)] (+ product margin))",
        ),
        (
            "fennel",
            "(let [product (* width height)] (+ product margin))",
        ),
    ] {
        let mut cmd = paredit();
        cmd.args([
            "refactor",
            "inline-let",
            "--dialect",
            dialect,
            "--path",
            "0",
            "--output",
            "json",
        ])
        .write_stdin(input)
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "\"dialect\": \"{dialect}\""
        )))
        .stdout(predicate::str::contains("\"binding_name\": \"product\""))
        .stdout(predicate::str::contains("(+ (* width height) margin)"));
    }
}

#[test]
fn cli_plans_inline_let_without_touching_shadowed_lambda_parameter() {
    let mut cmd = paredit();
    cmd.args([
        "refactor",
        "inline-let",
        "--path",
        "0",
        "--dialect",
        "common-lisp",
        "--output",
        "json",
    ])
    .write_stdin("(let ((x 1)) (list x (lambda (x) x)))")
    .assert()
    .success()
    .stdout(predicate::str::contains("\"binding_name\": \"x\""))
    .stdout(predicate::str::contains("\"reference_count\": 1"))
    .stdout(predicate::str::contains("(list 1 (lambda (x) x))"));
}
