//! The names `refactor verify` reports a rename cannot reach.
//!
//! Section I item I2. Every rename in this tool is syntactic: it rewrites the
//! atoms whose text is the symbol. A name assembled at macro-expansion time —
//! `(intern "HANDLE-CLICK")`, or `(intern (format nil "HANDLE-~a" kind))` — is
//! not one of those atoms, and a rename that reported "2 occurrences renamed"
//! while leaving both behind would have said something false.
//!
//! The analysis does not *follow* the construction. Doing so would mean
//! evaluating arbitrary Lisp at analysis time, which is the thing this tool
//! exists not to do. It reports the sites, so an incomplete rename is a
//! disclosed incompleteness.

use super::*;

use serde_json::Value;
use std::path::PathBuf;

fn workspace(label: &str, source: &str) -> PathBuf {
    let dir = fresh_temp_dir(label);
    fs::write(dir.join("core.lisp"), source).expect("write source");
    dir
}

fn verify(dir: &std::path::Path, symbol: &str) -> Value {
    let output = paredit()
        .current_dir(dir)
        .args(["refactor", "verify", "--symbol", symbol, "core.lisp"])
        .assert()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).expect("verification is JSON")
}

fn macro_check(report: &Value) -> Value {
    report["checks"]
        .as_array()
        .expect("checks")
        .iter()
        .find(|check| check["code"] == "macro-constructed-symbols")
        .expect("the macro-constructed-symbols check is present")
        .clone()
}

/// The motivating case: a call site whose symbol is a string literal.
#[test]
fn a_literal_naming_the_target_fails_the_check() {
    let dir = workspace(
        "macro-literal",
        "(defun handle-click (event)\n  event)\n\n(defun dispatch (x)\n  (funcall (intern \"HANDLE-CLICK\") x))\n",
    );
    let check = macro_check(&verify(&dir, "handle-click"));

    assert_eq!(check["passed"], false);
    assert_eq!(check["count"], 1);
    assert_eq!(
        check["level"], "warning",
        "a construction site is not proof the rename is wrong"
    );
    assert!(
        check["message"]
            .as_str()
            .is_some_and(|message| message.contains("names-target-literally")),
        "unexpected message: {check}"
    );
}

/// A macro that builds its definition's name from a format string.
#[test]
fn a_name_built_inside_a_macro_fails_the_check() {
    let dir = workspace(
        "macro-computed",
        "(defmacro define-handler (name)\n  `(defun ,(intern (format nil \"HANDLE-~a\" name)) (event) event))\n",
    );
    let check = macro_check(&verify(&dir, "handle-click"));

    assert_eq!(check["passed"], false);
    assert!(
        check["message"]
            .as_str()
            .is_some_and(|message| message.contains("computed-name")),
        "unexpected message: {check}"
    );
}

/// Ordinary code must pass, or the check is noise that gets turned off.
#[test]
fn code_that_constructs_no_symbols_passes() {
    let dir = workspace(
        "macro-clean",
        "(defun handle-click (event)\n  (process event))\n\n(defun dispatch (e)\n  (handle-click e))\n",
    );
    let check = macro_check(&verify(&dir, "handle-click"));

    assert_eq!(check["passed"], true);
    assert_eq!(check["count"], 0);
}

/// A literal about a different symbol says nothing about *this* rename.
#[test]
fn a_literal_naming_another_symbol_does_not_fail_the_check() {
    let dir = workspace(
        "macro-other",
        "(defun handle-click (event)\n  event)\n\n(defun other (x)\n  (funcall (intern \"SOMETHING-ELSE\") x))\n",
    );
    let check = macro_check(&verify(&dir, "handle-click"));

    assert_eq!(check["passed"], true, "unexpected finding: {check}");
}

/// The check names the file and line, because "somewhere in this project a
/// symbol is interned" is not actionable.
#[test]
fn the_check_names_the_file_and_line() {
    let dir = workspace(
        "macro-located",
        "(defun handle-click (e) e)\n\n\n(funcall (intern \"HANDLE-CLICK\") x)\n",
    );
    let message = macro_check(&verify(&dir, "handle-click"))["message"]
        .as_str()
        .expect("message")
        .to_owned();

    assert!(
        message.contains("core.lisp:4"),
        "unexpected message: {message}"
    );
}

/// A file with many sites must summarise rather than print forty of them, or
/// the check becomes a wall of text nobody reads.
#[test]
fn many_sites_are_summarised() {
    let mut source = String::from("(defun handle-click (e) e)\n");
    for index in 0..9 {
        source.push_str(&format!("(funcall (intern \"HANDLE-CLICK-{index}\") x)\n"));
    }
    let dir = workspace("macro-many", &source);
    let check = macro_check(&verify(&dir, "handle-click"));

    assert_eq!(check["count"], 9);
    assert!(
        check["message"]
            .as_str()
            .is_some_and(|message| message.contains("and 4 more")),
        "unexpected message: {check}"
    );
}
