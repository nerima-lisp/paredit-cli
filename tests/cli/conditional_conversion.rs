use super::*;

fn converted(command: &str, dialect: &str, input: &str, expected: &str) {
    let output = paredit()
        .args(["refactor", command, "--dialect", dialect, "--path", "0"])
        .write_stdin(input)
        .output()
        .expect("run conversion");
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains(expected));
}

#[test]
fn converts_all_four_conditional_forms() {
    converted(
        "convert-when-to-if",
        "common-lisp",
        "(when ok one two)",
        "(if ok (progn one two))",
    );
    // A one-form body needs no `progn`: an `if` branch already takes exactly
    // one form. Emitting it anyway generated code that `redundant-progn`
    // reports, and made `if ↔ unless` gain a level on every round trip.
    converted(
        "convert-unless-to-if",
        "emacs-lisp",
        "(unless ok no)",
        "(if ok nil no)",
    );
    converted(
        "convert-if-to-when",
        "common-lisp",
        "(if ok yes nil)",
        "(when ok yes)",
    );
    converted(
        "convert-if-to-unless",
        "emacs-lisp",
        "(if ok nil no)",
        "(unless ok no)",
    );
}

#[test]
fn rejects_non_nil_branches() {
    paredit()
        .args([
            "refactor",
            "convert-if-to-when",
            "--dialect",
            "common-lisp",
            "--path",
            "0",
        ])
        .write_stdin("(if ok yes no)")
        .assert()
        .failure();
    paredit()
        .args([
            "refactor",
            "convert-if-to-unless",
            "--dialect",
            "emacs-lisp",
            "--path",
            "0",
        ])
        .write_stdin("(if ok yes no)")
        .assert()
        .failure();
}
