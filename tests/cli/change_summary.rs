//! `inspect change`: what an edit changed, in a sentence.

use super::*;

use std::path::Path;

fn pair(name: &str, before: &str, after: &str) -> (PathBuf, PathBuf) {
    let dir = fresh_temp_dir(name);
    let before_path = dir.join("before.lisp");
    let after_path = dir.join("after.lisp");
    fs::write(&before_path, before).expect("write before");
    fs::write(&after_path, after).expect("write after");
    (before_path, after_path)
}

fn report(before: &Path, after: &Path, extra: &[&str]) -> serde_json::Value {
    let output = paredit()
        .args(["inspect", "change", "--before"])
        .arg(before)
        .args(["--after"])
        .arg(after)
        .args(extra)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).expect("valid JSON")
}

#[test]
fn an_identical_pair_reports_no_change() {
    let (before, after) = pair("change-same", "(defun f (x) x)\n", "(defun f (x) x)\n");
    let report = report(&before, &after, &[]);

    assert_eq!(report["identical"], true);
    assert_eq!(report["formatting_only"], false);
    assert!(report["changes"].as_array().expect("changes").is_empty());
    assert_eq!(
        report["headline"],
        "No change: the two versions are identical."
    );
}

/// The distinction that decides whether a review needs attention at all.
#[test]
fn a_whitespace_only_edit_is_reported_as_formatting() {
    let (before, after) = pair(
        "change-format",
        "(defun f (x) x)\n",
        "(defun f  (x)  x)\n\n",
    );
    let report = report(&before, &after, &[]);

    assert_eq!(report["identical"], false);
    assert_eq!(report["formatting_only"], true);
    assert!(report["changes"].as_array().expect("changes").is_empty());
}

/// The inference the whole command is for. "One removed, one added" loses the
/// only fact a reviewer wants.
#[test]
fn a_rename_is_reported_as_a_rename() {
    let (before, after) = pair(
        "change-rename",
        "(defun old-name (x) (list x))\n",
        "(defun new-name (x) (list x))\n",
    );
    let report = report(&before, &after, &[]);

    assert_eq!(report["counts"]["renamed"], 1);
    assert_eq!(report["counts"]["added"], 0);
    assert_eq!(report["counts"]["removed"], 0);

    let change = &report["changes"][0];
    assert_eq!(change["kind"], "renamed");
    assert_eq!(change["before"]["name"], "old-name");
    assert_eq!(change["after"]["name"], "new-name");
    assert!(
        change["sentence"]
            .as_str()
            .expect("sentence")
            .contains("Renamed `old-name` to `new-name`")
    );
}

/// A rename that also changed the body is two separate facts, and claiming
/// otherwise asserts an intent the evidence does not support.
#[test]
fn a_rename_with_a_changed_body_is_not_inferred() {
    let (before, after) = pair(
        "change-rename-modified",
        "(defun old-name (x) (list x))\n",
        "(defun new-name (x) (vector x))\n",
    );
    let report = report(&before, &after, &[]);

    assert_eq!(report["counts"]["renamed"], 0);
    assert_eq!(report["counts"]["added"], 1);
    assert_eq!(report["counts"]["removed"], 1);
}

#[test]
fn inserting_a_definition_reports_one_addition_and_the_moves() {
    let (before, after) = pair(
        "change-insert",
        "(defun f (x) x)\n(defun g (y) y)\n",
        "(defun new (z) z)\n(defun f (x) x)\n(defun g (y) y)\n",
    );
    let report = report(&before, &after, &[]);

    assert_eq!(report["counts"]["added"], 1);
    assert_eq!(report["counts"]["removed"], 0);
    assert_eq!(report["counts"]["moved"], 2);
    assert_eq!(report["definition_count"]["before"], 2);
    assert_eq!(report["definition_count"]["after"], 3);
}

/// The text output is the draft: a headline and one bullet per change, with
/// real line breaks rather than escaped ones.
#[test]
fn the_text_output_is_a_pull_request_draft() {
    let (before, after) = pair(
        "change-draft",
        "(defun a (x) x)\n(defun b (y) y)\n",
        "(defun a (x) (list x))\n(defun c (z) z)\n",
    );

    let output = paredit()
        .args(["inspect", "change", "--output", "text", "--before"])
        .arg(&before)
        .args(["--after"])
        .arg(&after)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let draft = String::from_utf8(output).expect("UTF-8");

    assert!(
        !draft.contains("\\u{a}"),
        "line breaks were escaped: {draft:?}"
    );
    let lines: Vec<&str> = draft.lines().filter(|line| !line.is_empty()).collect();
    assert!(lines[0].ends_with("definitions."), "{draft}");
    assert!(
        lines[1..].iter().all(|line| line.starts_with("- ")),
        "{draft}"
    );
}

/// The JSON carries both the sentence and the facts it was rendered from, so
/// a caller can paste one or compute with the other.
#[test]
fn the_json_carries_both_the_prose_and_the_facts() {
    let (before, after) = pair(
        "change-both",
        "(defun f (x) x)\n",
        "(defun f (x) (list x))\n",
    );
    let report = report(&before, &after, &[]);

    assert!(report["draft"].as_str().expect("draft").contains('\n'));
    assert_eq!(report["headline"], "1 modified definition.");
    assert_eq!(report["changes"][0]["after"]["line"], 1);
    assert_eq!(report["dialect"], "common-lisp");
}

/// The gate is on substance: reformatting a file must not fail it.
#[test]
fn fail_on_change_ignores_a_formatting_only_edit() {
    let (before, after) = pair(
        "change-gate-format",
        "(defun f (x) x)\n",
        "(defun f  (x)  x)\n",
    );
    paredit()
        .args(["inspect", "change", "--fail-on-change", "--before"])
        .arg(&before)
        .args(["--after"])
        .arg(&after)
        .assert()
        .success();
}

#[test]
fn fail_on_change_trips_on_a_real_change() {
    let (before, after) = pair(
        "change-gate-real",
        "(defun f (x) x)\n",
        "(defun f (x) (list x))\n",
    );
    paredit()
        .args(["inspect", "change", "--fail-on-change", "--before"])
        .arg(&before)
        .args(["--after"])
        .arg(&after)
        .assert()
        .code(3);
}

/// Comparing an unparsable version would produce a confident wrong answer.
#[test]
fn an_unbalanced_version_is_refused_with_what_to_run_next() {
    let (before, after) = pair("change-unbalanced", "(defun f (x) x)\n", "(defun f (x\n");
    paredit()
        .args(["inspect", "change", "--before"])
        .arg(&before)
        .args(["--after"])
        .arg(&after)
        .assert()
        .failure()
        .stderr(predicate::str::contains("paredit inspect check"));
}

/// The report is deterministic, like every other.
#[test]
fn the_report_is_byte_identical_across_runs() {
    let (before, after) = pair(
        "change-deterministic",
        "(defun a (x) x)\n(defun b (y) y)\n(defun c (z) z)\n",
        "(defun d (x) x)\n(defun b (y) (list y))\n",
    );
    let first = serde_json::to_string(&report(&before, &after, &[])).expect("serialize");
    let second = serde_json::to_string(&report(&before, &after, &[])).expect("serialize");
    assert_eq!(first, second);
}
