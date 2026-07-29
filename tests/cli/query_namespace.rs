//! The `query` namespace end to end.
//!
//! The unit tests in `paredit-core-syntax` pin what the rewriter does to a
//! string. These pin what the *command* does to a file: that nothing is
//! written without `--write`, that the skip counts reach every output format,
//! and that the two refusals survive the trip through the CLI.

use super::*;

fn workspace(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = fresh_temp_dir(name);
    for (file, source) in files {
        fs::write(dir.join(file), source).expect("write fixture");
    }
    dir
}

const ONE_ARMED: &str = "(defun foo (x)\n  (if (> x 1) (print x) nil))\n";

#[test]
fn find_reports_every_match_with_its_captures() {
    let dir = workspace("query-find", &[("a.lisp", ONE_ARMED)]);
    let output = paredit()
        .args(["query", "find", "--query", "(if ?t ?a nil)"])
        .arg(&dir)
        .output()
        .expect("run query find");
    assert!(output.status.success());

    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("query find emits json");
    assert_eq!(report["finding_count"], 1);
    let captures = &report["files"][0]["findings"][0]["captures"];
    let names: Vec<&str> = captures
        .as_array()
        .expect("captures")
        .iter()
        .map(|capture| capture["name"].as_str().expect("capture name"))
        .collect();
    assert_eq!(names, vec!["a", "t"]);
}

#[test]
fn find_gates_in_both_directions() {
    let dir = workspace("query-find-gate", &[("a.lisp", ONE_ARMED)]);

    // The shape is present: --fail-on-match trips, --fail-on-no-match does not.
    paredit()
        .args([
            "query",
            "find",
            "--query",
            "(if ?t ?a nil)",
            "--fail-on-match",
        ])
        .arg(&dir)
        .assert()
        .code(3);
    paredit()
        .args([
            "query",
            "find",
            "--query",
            "(if ?t ?a nil)",
            "--fail-on-no-match",
        ])
        .arg(&dir)
        .assert()
        .success();

    // A shape that is absent inverts both answers.
    paredit()
        .args(["query", "find", "--query", "(loop ...)", "--fail-on-match"])
        .arg(&dir)
        .assert()
        .success();
    paredit()
        .args([
            "query",
            "find",
            "--query",
            "(loop ...)",
            "--fail-on-no-match",
        ])
        .arg(&dir)
        .assert()
        .code(3);
}

#[test]
fn count_reports_one_column_per_pattern_in_command_line_order() {
    let dir = workspace("query-count", &[("a.lisp", ONE_ARMED)]);
    let output = paredit()
        .args([
            "query",
            "count",
            "--query",
            "(if ?t ?a nil)",
            "--query",
            "(when ?t ?a)",
        ])
        .arg(&dir)
        .output()
        .expect("run query count");
    assert!(output.status.success());

    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("query count emits json");
    let patterns = report["patterns"].as_array().expect("patterns");
    assert_eq!(patterns[0]["query"], "(if ?t ?a nil)");
    assert_eq!(patterns[0]["count"], 1);
    assert_eq!(patterns[1]["query"], "(when ?t ?a)");
    assert_eq!(patterns[1]["count"], 0);
    assert_eq!(report["total"], 1);
}

#[test]
fn replace_writes_nothing_without_write() {
    let dir = workspace("query-replace-dry", &[("a.lisp", ONE_ARMED)]);
    paredit()
        .args([
            "query",
            "replace",
            "--query",
            "(if ?t ?a nil)",
            "--rewrite",
            "(when ?t ?a)",
        ])
        .arg(&dir)
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(dir.join("a.lisp")).expect("read back"),
        ONE_ARMED,
        "the default must be a plan, not an edit"
    );
}

#[test]
fn replace_writes_with_write_and_the_result_still_parses() {
    let dir = workspace("query-replace-write", &[("a.lisp", ONE_ARMED)]);
    paredit()
        .args([
            "query",
            "replace",
            "--query",
            "(if ?t ?a nil)",
            "--rewrite",
            "(when ?t ?a)",
            "--write",
        ])
        .arg(&dir)
        .assert()
        .success();

    let rewritten = fs::read_to_string(dir.join("a.lisp")).expect("read back");
    assert_eq!(rewritten, "(defun foo (x)\n  (when (> x 1) (print x)))\n");
    paredit()
        .args(["inspect", "check", "--file"])
        .arg(dir.join("a.lisp"))
        .assert()
        .success();
}

#[test]
fn replace_check_gates_without_writing() {
    let dir = workspace("query-replace-check", &[("a.lisp", ONE_ARMED)]);
    paredit()
        .args([
            "query",
            "replace",
            "--query",
            "(if ?t ?a nil)",
            "--rewrite",
            "(when ?t ?a)",
            "--check",
        ])
        .arg(&dir)
        .assert()
        .code(3);
    assert_eq!(
        fs::read_to_string(dir.join("a.lisp")).expect("read back"),
        ONE_ARMED
    );
}

/// A comment inside the match that no capture carries is *deleted* by the
/// splice, and the result still parses — so the reparse guard cannot catch
/// it. This is the guard that can.
#[test]
fn replace_refuses_a_rewrite_that_would_delete_a_comment() {
    let source = "(if a\n    b ; keep me\n    nil)\n";
    let dir = workspace("query-replace-comment", &[("a.lisp", source)]);
    let output = paredit()
        .args([
            "query",
            "replace",
            "--query",
            "(if ?t ?a nil)",
            "--rewrite",
            "(when ?t ?a)",
            "--write",
        ])
        .arg(&dir)
        .output()
        .expect("run query replace");
    assert!(output.status.success());

    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json");
    assert_eq!(report["summary"]["replacements"], 0);
    assert_eq!(report["summary"]["skipped"], 1);
    let reasons = report["skippedByReason"].as_array().expect("reasons");
    let comment_loss = reasons
        .iter()
        .find(|reason| reason["reason"] == "comment-loss")
        .expect("comment-loss is always reported, including as a zero");
    assert_eq!(comment_loss["count"], 1);

    assert_eq!(
        fs::read_to_string(dir.join("a.lisp")).expect("read back"),
        source,
        "a refused match must leave the file alone"
    );
}

#[test]
fn allow_comment_loss_is_what_lets_that_rewrite_through() {
    let source = "(if a\n    b ; drop me\n    nil)\n";
    let dir = workspace("query-replace-allow", &[("a.lisp", source)]);
    paredit()
        .args([
            "query",
            "replace",
            "--query",
            "(if ?t ?a nil)",
            "--rewrite",
            "(when ?t ?a)",
            "--allow-comment-loss",
            "--write",
        ])
        .arg(&dir)
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(dir.join("a.lisp")).expect("read back"),
        "(when a b)\n"
    );
}

#[test]
fn a_template_naming_an_unbound_capture_fails_before_reading_any_file() {
    let dir = workspace("query-replace-unbound", &[("a.lisp", ONE_ARMED)]);
    paredit()
        .args([
            "query",
            "replace",
            "--query",
            "(if ?t ?a nil)",
            "--rewrite",
            "(when ?t ?missing)",
        ])
        .arg(&dir)
        .assert()
        .failure()
        .stderr(predicate::str::contains("?missing"));
}

/// A captured double float re-serialized through a printer becomes `1`, and a
/// captured string's `\n` becomes the letter `n`. Splicing source text cannot
/// do either, and this is the end-to-end pin on that.
#[test]
fn a_rewrite_reproduces_captured_literals_byte_for_byte() {
    let source = "(old 1.0d0 \"a\\nb\" #\\Space)\n";
    let dir = workspace("query-replace-literals", &[("a.lisp", source)]);
    paredit()
        .args([
            "query",
            "replace",
            "--query",
            "(old ?args...)",
            "--rewrite",
            "(new ?args...)",
            "--write",
        ])
        .arg(&dir)
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(dir.join("a.lisp")).expect("read back"),
        "(new 1.0d0 \"a\\nb\" #\\Space)\n"
    );
}
