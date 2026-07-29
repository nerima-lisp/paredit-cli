//! `refactor fold-constants`: the write side of `inspect constants`.
//!
//! Every test here is about a rewrite that still parses and is still wrong.
//! The write path's balance check catches an unbalanced result and nothing
//! else, so a fold that changes what a form *means* — quoted data rewritten
//! as if it were code, a string escape a Lisp reader spells differently than
//! Rust does, a float that prints as an integer — passes every structural
//! check on the way out. Only comparing the folded text against what the
//! reader will make of it catches those.

use super::*;

const FIXTURE: &str = concat!(
    "(defun plain () (+ 1 2))\n",
    "(defun quoted () '(+ 1 2))\n",
    "(defun in-list () (list '(+ 1 2) (+ 3 4)))\n",
    "(defmacro m () `(+ 1 2))\n",
);

fn fixture(name: &str) -> std::path::PathBuf {
    written_fixture(name, FIXTURE)
}

fn written_fixture(name: &str, source: &str) -> std::path::PathBuf {
    let dir = fresh_temp_dir(name);
    let file = dir.join("source.lisp");
    fs::write(&file, source).expect("write fixture");
    file
}

/// Folds `source` in place and returns what the file holds afterwards.
fn fold_in_place(name: &str, source: &str) -> String {
    let file = written_fixture(name, source);
    paredit()
        .args(["refactor", "fold-constants", "--write"])
        .arg(&file)
        .assert()
        .success();
    fs::read_to_string(&file).expect("read rewritten fixture")
}

#[test]
fn cli_folds_live_arithmetic_and_leaves_quoted_forms_alone() {
    let file = fixture("fold-constants-write");
    paredit()
        .args(["refactor", "fold-constants", "--write"])
        .arg(&file)
        .assert()
        .success();

    let rewritten = fs::read_to_string(&file).expect("read rewritten fixture");
    assert_eq!(
        rewritten,
        concat!(
            "(defun plain () 3)\n",
            "(defun quoted () '(+ 1 2))\n",
            "(defun in-list () (list '(+ 1 2) 7))\n",
            "(defmacro m () `(+ 1 2))\n",
        )
    );
}

#[test]
fn cli_plans_without_writing() {
    let file = fixture("fold-constants-plan");
    let output = paredit()
        .args(["refactor", "fold-constants"])
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).expect("valid JSON");

    assert_eq!(report["fold_count"], 2);
    assert_eq!(report["saved_bytes"], 12);
    assert_eq!(report["files"][0]["changed"], true);
    assert_eq!(
        fs::read_to_string(&file).expect("read fixture"),
        FIXTURE,
        "planning must not write"
    );
}

#[test]
fn cli_min_saved_bytes_holds_back_the_unprofitable_folds() {
    let file = fixture("fold-constants-threshold");
    let output = paredit()
        .args(["refactor", "fold-constants", "--min-saved-bytes", "99"])
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).expect("valid JSON");
    assert_eq!(report["fold_count"], 0);
    assert_eq!(report["files"][0]["changed"], false);
}

#[test]
fn cli_folded_output_still_parses() {
    let file = fixture("fold-constants-reparse");
    paredit()
        .args(["refactor", "fold-constants", "--write"])
        .arg(&file)
        .assert()
        .success();

    // The write path refuses an unbalanced rewrite, so reaching here already
    // proves it; asserting explicitly makes the intent legible.
    paredit()
        .args(["inspect", "check"])
        .arg("--file")
        .arg(&file)
        .assert()
        .success();
}

#[test]
fn cli_leaves_arithmetic_nested_inside_quoted_data_alone() {
    // `'(a (+ 1 2))` is a two-element list whose second element is the list
    // `(+ 1 2)`. Folding it to `'(a 3)` deletes two elements of a data
    // literal. The rewrite still parses, so nothing downstream catches it.
    let source = concat!(
        "(defun f () '(a (+ 1 2)))\n",
        "(defmacro g () `(list (+ 5 6)))\n",
        "(defun h () '(a (b (c (+ 7 8)))))\n",
    );
    assert_eq!(fold_in_place("fold-constants-nested-quote", source), source);
}

#[test]
fn cli_folds_a_string_in_lisp_escaping_not_rust_escaping() {
    // A Lisp reader knows two escapes inside `"…"`: `\\` and `\"`. Rust's
    // `{:?}` writes a newline as `\n`, which the reader takes as the letter
    // `n` — the string reparses, one character shorter and different.
    let source = "(defun s () (if t \"line1\nline2 \\\\ q\\\" end\" 2))\n";
    let folded = fold_in_place("fold-constants-string-escapes", source);
    assert_eq!(folded, "(defun s () \"line1\nline2 \\\\ q\\\" end\")\n");
    assert!(
        !folded.contains("\\n"),
        "a real newline must stay a real newline, not become `\\n`: {folded:?}"
    );
}

#[test]
fn cli_refuses_to_fold_a_float() {
    // `1.0d0` is a `double-float`. The value layer keeps only its `f64`, so
    // the marker that says which float type it is cannot be reproduced —
    // before this was refused, the fold wrote the integer `1`.
    let source = "(defun h () (if t 1.0d0 2))\n";
    assert_eq!(fold_in_place("fold-constants-float", source), source);
}

#[test]
fn cli_reports_no_fold_for_a_float_or_a_quoted_form() {
    let file = written_fixture(
        "fold-constants-float-plan",
        "(defun h () (if t 1.0d0 2))\n(defun f () '(a (+ 1 2)))\n",
    );
    let output = paredit()
        .args(["refactor", "fold-constants"])
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).expect("valid JSON");
    assert_eq!(report["fold_count"], 0, "{report}");
    assert_eq!(report["files"][0]["changed"], false);
}
