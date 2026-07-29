//! `refactor add-ignore-declaration`: the write side of `inspect
//! unused-parameters`.
//!
//! The core-level tests cover where the declaration lands; these cover what
//! only the binary can prove — that the plan, the diff and the write agree,
//! and that applying it actually clears the report that asked for it.

use super::*;

const FIXTURE: &str = concat!(
    "(defun already (x y)\n  (declare (ignore y))\n  x)\n\n",
    "(defun plain (x y)\n  x)\n\n",
    "(defun documented (x y)\n  \"A docstring.\"\n  x)\n\n",
    "(defun typed (x y)\n  (declare (type fixnum x))\n  x)\n",
);

fn fixture(name: &str) -> std::path::PathBuf {
    let dir = fresh_temp_dir(name);
    let file = dir.join("source.lisp");
    fs::write(&file, FIXTURE).expect("write fixture");
    file
}

#[test]
fn cli_plans_one_declaration_per_definition_without_writing() {
    let file = fixture("add-ignore-plan");
    let output = paredit()
        .args(["refactor", "add-ignore-declaration"])
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value =
        serde_json::from_slice(&output).expect("the plan is valid JSON");

    // `already` is absent: the report counts the name's appearance inside its
    // existing declaration as a reference, so it is never flagged.
    assert_eq!(report["declaration_count"], 3);
    assert_eq!(report["parameter_count"], 3);
    let names: Vec<&str> = report["files"][0]["declarations"]
        .as_array()
        .expect("declarations")
        .iter()
        .map(|item| item["definition_name"].as_str().expect("name"))
        .collect();
    assert_eq!(names, ["plain", "documented", "typed"]);

    assert_eq!(
        fs::read_to_string(&file).expect("read fixture"),
        FIXTURE,
        "planning must not write"
    );
}

#[test]
fn cli_writes_the_declarations_and_clears_the_report() {
    let file = fixture("add-ignore-write");
    paredit()
        .args(["refactor", "add-ignore-declaration", "--write"])
        .arg(&file)
        .assert()
        .success();

    let rewritten = fs::read_to_string(&file).expect("read rewritten fixture");
    assert!(rewritten.contains("(defun plain (x y)\n  (declare (ignore y))\n  x)"));
    // The docstring keeps its place above the declaration.
    assert!(
        rewritten.contains("(defun documented (x y)\n  \"A docstring.\"\n  (declare (ignore y))")
    );
    // An existing declaration is followed, not merged into.
    assert!(
        rewritten.contains(
            "(defun typed (x y)\n  (declare (type fixnum x))\n  (declare (ignore y))\n  x)"
        )
    );

    // The whole point: the report that asked for this now reports nothing.
    let after = paredit()
        .args(["inspect", "unused-parameters", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&after).expect("report is valid JSON");
    assert_eq!(report["unused_parameter_count"], 0);
}

#[test]
fn cli_diff_shows_only_the_inserted_declarations() {
    let file = fixture("add-ignore-diff");
    let output = paredit()
        .args(["refactor", "add-ignore-declaration", "--diff"])
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let diff = String::from_utf8(output).expect("diff is utf8");
    assert_eq!(diff.matches("+  (declare (ignore y))").count(), 3);
    assert_eq!(
        fs::read_to_string(&file).expect("read fixture"),
        FIXTURE,
        "--diff must not write"
    );
}

#[test]
fn cli_keeps_a_single_line_definition_on_one_line() {
    // Every fixture above is multi-line, which is what hid this: the insertion
    // assumed its offset began a line and wrote
    // `(defun f (x y) (declare (ignore y))\nx)`, stranding the body at column
    // zero.
    let dir = fresh_temp_dir("add-ignore-single-line");
    let file = dir.join("source.lisp");
    fs::write(&file, "(defun f (x y) x)\n").expect("write fixture");

    paredit()
        .args(["refactor", "add-ignore-declaration", "--write"])
        .arg(&file)
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(&file).expect("read rewritten fixture"),
        "(defun f (x y) (declare (ignore y)) x)\n"
    );

    let after = paredit()
        .args(["inspect", "unused-parameters", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&after).expect("report is valid JSON");
    assert_eq!(report["unused_parameter_count"], 0);
}

#[test]
fn cli_plans_nothing_for_a_dialect_without_declare() {
    let dir = fresh_temp_dir("add-ignore-clojure");
    let file = dir.join("source.clj");
    fs::write(&file, "(defn f [x y] x)\n").expect("write fixture");

    let output = paredit()
        .args(["refactor", "add-ignore-declaration"])
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).expect("valid JSON");
    // A workspace sweep should skip a Clojure file, not stop at it.
    assert_eq!(report["declaration_count"], 0);
}
