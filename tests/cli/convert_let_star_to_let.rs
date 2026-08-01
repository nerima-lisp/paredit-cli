use super::*;

#[test]
fn converts_independent_let_star() {
    let output = paredit()
        .args([
            "refactor",
            "convert-let-star-to-let",
            "--dialect",
            "common-lisp",
            "--path",
            "0",
        ])
        .write_stdin("(let* ((x (first)) (y (second))) (+ x y))")
        .output()
        .expect("run conversion");
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout");
    assert!(stdout.contains("(let ((x (first)) (y (second))) (+ x y))"));
    assert!(stdout.contains("\"binding_names\": ["));
}

#[test]
fn rejects_initializer_dependency() {
    paredit()
        .args([
            "refactor",
            "convert-let-star-to-let",
            "--dialect",
            "common-lisp",
            "--path",
            "0",
        ])
        .write_stdin("(let* ((x 1) (y (+ x 2))) y)")
        .assert()
        .failure()
        .stderr(predicate::str::contains("references earlier binding 'x'"));
}

#[test]
fn allow_partial_splits_instead_of_refusing() {
    let output = paredit()
        .args([
            "refactor",
            "convert-let-star-to-let",
            "--dialect",
            "common-lisp",
            "--path",
            "0",
            "--allow-partial",
        ])
        .write_stdin("(let* ((x 1) (y (+ x 2))) y)")
        .output()
        .expect("run conversion");
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout");
    assert!(stdout.contains("(let ((x 1)) (let* ((y (+ x 2))) y))"));
    assert!(stdout.contains("\"partial\": true"));
}

#[test]
fn allow_partial_matches_full_conversion_when_no_dependency_exists() {
    let with_flag = paredit()
        .args([
            "refactor",
            "convert-let-star-to-let",
            "--dialect",
            "common-lisp",
            "--path",
            "0",
            "--allow-partial",
        ])
        .write_stdin("(let* ((x (first)) (y (second))) (+ x y))")
        .output()
        .expect("run conversion");
    assert!(
        with_flag.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&with_flag.stderr)
    );
    let stdout = String::from_utf8(with_flag.stdout).expect("stdout");
    assert!(stdout.contains("(let ((x (first)) (y (second))) (+ x y))"));
    assert!(stdout.contains("\"partial\": false"));
}

#[test]
fn allow_partial_reports_partial_field_in_text_output() {
    paredit()
        .args([
            "refactor",
            "convert-let-star-to-let",
            "--dialect",
            "common-lisp",
            "--path",
            "0",
            "--allow-partial",
            "--output",
            "text",
        ])
        .write_stdin("(let* ((x 1) (y (+ x 2))) y)")
        .assert()
        .success()
        .stdout(predicate::str::contains("partial\ttrue"));
}
