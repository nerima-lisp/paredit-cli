//! `refactor fold-constants`: the write side of `inspect constants`.
//!
//! The safety property worth testing at this level is the one the command
//! does not implement itself — quoted forms are never folded, because the
//! value layer refuses to evaluate through `'` and `` ` ``. A regression
//! there would turn `'(+ 1 2)` from a list into the number 3.

use super::*;

const FIXTURE: &str = concat!(
    "(defun plain () (+ 1 2))\n",
    "(defun quoted () '(+ 1 2))\n",
    "(defun in-list () (list '(+ 1 2) (+ 3 4)))\n",
    "(defmacro m () `(+ 1 2))\n",
);

fn fixture(name: &str) -> std::path::PathBuf {
    let dir = fresh_temp_dir(name);
    let file = dir.join("source.lisp");
    fs::write(&file, FIXTURE).expect("write fixture");
    file
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
