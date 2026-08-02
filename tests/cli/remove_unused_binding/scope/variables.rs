use super::*;

#[test]
fn cli_plans_remove_unused_binding_ignoring_shadowed_lambda_parameter() {
    let mut cmd = paredit();
    cmd.args([
        "refactor",
        "remove-unused-binding",
        "--dialect",
        "common-lisp",
        "--path",
        "0",
        "--name",
        "x",
        "--output",
        "json",
    ])
    .write_stdin("(let ((x 1) (used 2)) (list used (lambda (x) x)))")
    .assert()
    .success()
    .stdout(predicate::str::contains("\"binding_name\": \"x\""))
    .stdout(predicate::str::contains("\"reference_count\": 0"))
    // `used` and `(lambda ...)` are `list`'s siblings, so they share the
    // column `list` itself starts at, one past the opening delimiter.
    .stdout(predicate::str::contains(
        "(let ((used 2))\\n  (list\\n   used\\n   (lambda (x)\\n     x)))",
    ));
}

#[test]
fn cli_rejects_remove_unused_let_star_binding_used_later() {
    let mut cmd = paredit();
    cmd.args([
        "refactor",
        "remove-unused-binding",
        "--dialect",
        "common-lisp",
        "--path",
        "0",
        "--name",
        "x",
    ])
    .write_stdin("(let* ((x 1) (y (+ x 2))) y)")
    .assert()
    .failure()
    .stderr(predicate::str::contains(
        "remove-unused-binding requires zero in-scope references",
    ));
}

#[test]
fn cli_keeps_let_star_binding_used_by_later_binding_in_all_bindings() {
    let mut cmd = paredit();
    cmd.args([
        "refactor",
        "remove-unused-binding",
        "--dialect",
        "common-lisp",
        "--path",
        "0",
        "--all-bindings",
        "--output",
        "json",
    ])
    .write_stdin("(let* ((x 1) (unused 2) (y (+ x 3))) y)")
    .assert()
    .success()
    .stdout(predicate::str::contains("\"binding_count\": 1"))
    .stdout(predicate::str::contains("\"binding_name\": \"unused\""))
    .stdout(predicate::str::contains(
        "\"replacement\": \"(let* ((x 1)\\n       (y (+ x 3)))\\n  y)\"",
    ));
}
