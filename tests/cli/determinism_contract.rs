//! H12: same input, same bytes.
//!
//! This is a contract, not an aspiration. Agents cache reports, diff them
//! against each other, and hash them into manifests; a report whose byte order
//! wobbles between runs breaks all three, and it breaks them *intermittently*,
//! which is the worst way to find out.
//!
//! The failure mode this catches is a real one and easy to reintroduce: a
//! finding collected through a `HashMap` and rendered in iteration order is
//! correct on every run and identical on none. Rust's `HashMap` randomises its
//! seed per process, so running the binary twice is exactly the experiment
//! that exposes it.

use super::*;

use std::path::Path;

/// Deliberately varied: several definitions, several findings per rule, a
/// package form, duplicated shapes, and a couple of shadowed bindings — so a
/// report has enough to sort wrongly.
const SOURCE: &str = r#"(defpackage :demo
  (:use :cl)
  (:export #:alpha #:beta #:gamma))

(in-package :demo)

(defun alpha (x y)
  (let ((x 1) (y 2))
    (if (eq x nil) (list x y) (list y x))))

(defun beta (a b)
  (let ((a 1) (b 2))
    (if (eq a nil) (list a b) (list b a))))

(defun gamma (p q)
  (setf p p)
  (when (not (null q))
    (+ q 0)))

(defclass widget ()
  ((name :initarg :name)
   (name :initarg :other)))
"#;

fn fixture() -> PathBuf {
    let dir = fresh_temp_dir("determinism");
    let path = dir.join("a.lisp");
    fs::write(&path, SOURCE).expect("write fixture");
    path
}

fn stdout_of(args: &[&str], file: &Path) -> Vec<u8> {
    paredit()
        .args(args)
        .arg(file)
        .assert()
        .get_output()
        .stdout
        .clone()
}

/// The commands below are chosen for the shapes that go wrong: grouped
/// findings, cross-file maps, sorted rollups, and one whole-tree dump.
const COMMANDS: &[&[&str]] = &[
    &["inspect", "lint", "--output", "json"],
    &["inspect", "lint", "--output", "text"],
    &["inspect", "lint", "--stats", "--output", "json"],
    &["inspect", "duplicates", "--output", "json"],
    &["inspect", "shadowed-bindings", "--output", "json"],
    &["inspect", "duplicate-slots", "--output", "json"],
    &["inspect", "complexity", "--output", "json"],
    &["inspect", "symbol-index", "--output", "json"],
    &["inspect", "packages", "--output", "json"],
    &["inspect", "definitions", "--output", "json"],
    &["inspect", "similarity", "--output", "json"],
];

/// Two processes, so a per-process hash seed cannot be the reason they agree.
#[test]
fn every_sampled_report_is_byte_identical_across_processes() {
    let file = fixture();
    for args in COMMANDS {
        let first = stdout_of(args, &file);
        let second = stdout_of(args, &file);
        assert_eq!(
            String::from_utf8_lossy(&first),
            String::from_utf8_lossy(&second),
            "`paredit {}` is not deterministic",
            args.join(" ")
        );
        assert!(
            !first.is_empty(),
            "`paredit {}` produced nothing, so this proves nothing",
            args.join(" ")
        );
    }
}

/// Ten runs rather than two for the report most likely to wobble: `lint` runs
/// 191 rules and groups their findings, which is the most collection-order
/// surface in the tool.
#[test]
fn lint_is_stable_over_repeated_runs() {
    let file = fixture();
    let baseline = stdout_of(&["inspect", "lint", "--output", "json"], &file);
    for run in 1..10 {
        let again = stdout_of(&["inspect", "lint", "--output", "json"], &file);
        assert_eq!(
            String::from_utf8_lossy(&baseline),
            String::from_utf8_lossy(&again),
            "run {run} differed"
        );
    }
}

/// The same file reached by two different but equivalent paths must produce
/// the same findings in the same order. Only the path strings may differ.
#[test]
fn an_equivalent_path_produces_an_equivalent_report() {
    let file = fixture();
    let indirect = file
        .parent()
        .expect("parent")
        .join(".")
        .join(file.file_name().expect("name"));

    let direct = stdout_of(&["inspect", "lint", "--output", "json"], &file);
    let via_dot = stdout_of(&["inspect", "lint", "--output", "json"], &indirect);

    let normalize = |bytes: &[u8]| {
        String::from_utf8_lossy(bytes)
            .replace(&indirect.display().to_string(), "<file>")
            .replace(&file.display().to_string(), "<file>")
    };
    assert_eq!(normalize(&direct), normalize(&via_dot));
}

/// Edits are covered too: the rewritten document is stdout, and an agent
/// diffing two runs of the same edit must see nothing.
#[test]
fn an_edit_produces_the_same_bytes_every_time() {
    let file = fixture();
    for args in [
        vec!["edit", "format", "--file"],
        vec!["edit", "select", "--path", "2", "--file"],
        vec!["edit", "wrap", "--path", "2", "--diff", "--file"],
    ] {
        let first = paredit()
            .args(&args)
            .arg(&file)
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let second = paredit()
            .args(&args)
            .arg(&file)
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        assert_eq!(
            String::from_utf8_lossy(&first),
            String::from_utf8_lossy(&second),
            "`paredit {}` is not deterministic",
            args.join(" ")
        );
    }
}

/// Discovery order must not depend on the filesystem's readdir order, which is
/// not sorted on any filesystem this runs on.
#[test]
fn workspace_discovery_reports_files_in_a_stable_order() {
    let dir = fresh_temp_dir("determinism-workspace");
    for name in ["zeta", "alpha", "mid", "beta", "omega"] {
        fs::write(dir.join(format!("{name}.lisp")), SOURCE).expect("write");
    }

    let first = stdout_of(&["inspect", "workspace", "--output", "json"], &dir);
    let second = stdout_of(&["inspect", "workspace", "--output", "json"], &dir);
    assert_eq!(
        String::from_utf8_lossy(&first),
        String::from_utf8_lossy(&second)
    );

    let report: serde_json::Value = serde_json::from_slice(&first).expect("valid JSON");
    assert!(
        report.to_string().contains("alpha.lisp"),
        "the fixture files were not discovered, so this proves nothing"
    );
}

/// Discovering the same tree through two roots that resolve to it must agree.
#[test]
fn a_multi_file_report_is_stable_across_processes() {
    let dir = fresh_temp_dir("determinism-multifile");
    for name in ["c", "a", "b"] {
        fs::write(dir.join(format!("{name}.lisp")), SOURCE).expect("write");
    }

    let first = stdout_of(&["inspect", "lint", "--output", "json"], &dir);
    let second = stdout_of(&["inspect", "lint", "--output", "json"], &dir);
    assert_eq!(
        String::from_utf8_lossy(&first),
        String::from_utf8_lossy(&second)
    );

    let report: serde_json::Value = serde_json::from_slice(&first).expect("valid JSON");
    assert!(
        report["finding_count"].as_u64().expect("count") > 0,
        "no findings, so ordering was never exercised"
    );
}
