//! End-to-end coverage for the selector surface every target-taking command
//! shares: `--query`, `--name`, `--line-column`, `--id`, `--from`/`--to`,
//! `--all`, and the relative moves.
//!
//! These run through the real binary rather than the resolver's unit tests
//! because the thing being checked here is that the selectors reach the
//! commands at all — the clap flattening, the dialect the pattern is read
//! with, and the refusals a caller actually sees.

use super::*;

const SOURCE: &str = "\
(defpackage :demo (:use :cl))
(in-package :demo)

(defun parse-header (stream)
  (let ((line (read-line stream)))
    (cleanup line)
    (cleanup line)))

(defun write-header (stream value)
  (cleanup value)
  (format stream \"~a\" value))
";

fn select(args: &[&str]) -> assert_cmd::assert::Assert {
    let mut command = paredit();
    command.args(["edit", "select"]);
    command.args(args);
    command.write_stdin(SOURCE).assert()
}

fn resolve(args: &[&str]) -> assert_cmd::assert::Assert {
    let mut command = paredit();
    command.args(["inspect", "resolve", "--dialect", "common-lisp"]);
    command.args(args);
    command.write_stdin(SOURCE).assert()
}

// --- L1 `--query` -------------------------------------------------------

#[test]
fn a_query_selects_the_form_its_pattern_matches() {
    select(&["--dialect", "common-lisp", "--query", "(format ...)"])
        .success()
        .stdout("(format stream \"~a\" value)");
}

#[test]
fn a_query_capture_selects_the_bound_subform() {
    select(&[
        "--dialect",
        "common-lisp",
        "--query",
        "(defun ?name ...)",
        "--capture",
        "name",
        "--all",
    ])
    .success()
    .stdout("parse-header\nwrite-header");
}

#[test]
fn a_repeated_capture_constrains_the_match() {
    let mut command = paredit();
    command
        .args([
            "edit",
            "select",
            "--dialect",
            "common-lisp",
            "--query",
            "(eq ?x ?x)",
        ])
        .write_stdin("(eq a b) (eq c c)")
        .assert()
        .success()
        .stdout("(eq c c)");
}

#[test]
fn a_malformed_query_reports_the_readers_own_message() {
    select(&["--dialect", "common-lisp", "--query", "(defun"])
        .failure()
        .stderr(predicate::str::contains(
            "pattern does not read as an S-expression: unclosed list starting at byte 0",
        ));
}

// --- L2 `--name` --------------------------------------------------------

#[test]
fn a_name_selects_the_definition_that_carries_it() {
    select(&["--dialect", "common-lisp", "--name", "write-header"])
        .success()
        .stdout(predicate::str::starts_with("(defun write-header"));
}

#[test]
fn an_unknown_name_names_the_selector_that_failed() {
    select(&["--dialect", "common-lisp", "--name", "absent"])
        .failure()
        .stderr(predicate::str::contains("no form matches --name absent"));
}

// --- L3 `--line-column` -------------------------------------------------

#[test]
fn a_line_and_column_select_the_smallest_form_there() {
    select(&["--dialect", "common-lisp", "--line-column", "6:5"])
        .success()
        .stdout("(cleanup line)");
}

#[test]
fn a_bare_line_defaults_to_the_first_column() {
    select(&["--dialect", "common-lisp", "--line-column", "9"])
        .success()
        .stdout(predicate::str::starts_with("(defun write-header"));
}

#[test]
fn a_line_past_the_end_of_the_input_is_refused() {
    select(&["--dialect", "common-lisp", "--line-column", "999"])
        .failure()
        .stderr(predicate::str::contains("is past the end of the input"));
}

// --- L4 `--from` / `--to` -----------------------------------------------

#[test]
fn a_range_selects_every_sibling_between_its_ends() {
    select(&[
        "--dialect",
        "common-lisp",
        "--from",
        "name:parse-header",
        "--to",
        "name:write-header",
    ])
    .success()
    .stdout(predicate::str::starts_with("(defun parse-header"))
    .stdout(predicate::str::ends_with("value))"));
}

#[test]
fn an_edit_refuses_a_range_rather_than_editing_its_first_form() {
    let mut command = paredit();
    command
        .args([
            "edit",
            "kill",
            "--dialect",
            "common-lisp",
            "--from",
            "name:parse-header",
            "--to",
            "name:write-header",
        ])
        .write_stdin(SOURCE)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "this edit acts on one form; --from/--to selects a range of 2 forms",
        ));
}

// --- L5 `--all` ---------------------------------------------------------

#[test]
fn an_ambiguous_selector_is_refused_and_says_how_to_proceed() {
    select(&["--dialect", "common-lisp", "--query", "(cleanup ?x)"])
        .failure()
        .stderr(predicate::str::contains("matches 3 forms; pass --all"));
}

#[test]
fn all_applies_one_edit_to_every_match() {
    let mut command = paredit();
    let output = command
        .args([
            "edit",
            "kill",
            "--dialect",
            "common-lisp",
            "--query",
            "(cleanup ?x)",
            "--all",
        ])
        .write_stdin(SOURCE)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let rewritten = String::from_utf8(output).expect("utf-8 output");
    assert!(
        !rewritten.contains("cleanup"),
        "every match should be gone:\n{rewritten}"
    );

    // The result must still be a balanced document: `--all` reparses between
    // applications precisely so a later edit cannot land on stale text.
    paredit()
        .args(["inspect", "check", "--dialect", "common-lisp"])
        .write_stdin(rewritten)
        .assert()
        .success();
}

/// `--all` only means something where the command can fan out. On one that
/// cannot, it must not quietly become "act on the first of three".
#[test]
fn all_does_not_silently_narrow_a_single_form_command() {
    let mut command = paredit();
    command
        .args([
            "edit",
            "replace",
            "--dialect",
            "common-lisp",
            "--query",
            "(cleanup ?x)",
            "--all",
            "--with",
            "(noop)",
        ])
        .write_stdin(SOURCE)
        .assert()
        .failure()
        .stderr(predicate::str::contains("matches 3 forms"));

    paredit()
        .args([
            "inspect",
            "form",
            "--dialect",
            "common-lisp",
            "--query",
            "(cleanup ?x)",
            "--all",
        ])
        .write_stdin(SOURCE)
        .assert()
        .failure()
        .stderr(predicate::str::contains("matches 3 forms"));
}

// --- L6 relative selectors ----------------------------------------------

#[test]
fn relative_moves_climb_descend_and_step_sideways() {
    select(&[
        "--dialect",
        "common-lisp",
        "--name",
        "parse-header",
        "--child",
        "1",
    ])
    .success()
    .stdout("parse-header");
    select(&[
        "--dialect",
        "common-lisp",
        "--line-column",
        "6:5",
        "--sibling",
        "1",
    ])
    .success()
    .stdout("(cleanup line)");
    select(&[
        "--dialect",
        "common-lisp",
        "--line-column",
        "6:5",
        "--parent",
    ])
    .success()
    .stdout(predicate::str::starts_with("(let (("));
}

#[test]
fn climbing_above_the_top_level_is_refused() {
    select(&[
        "--dialect",
        "common-lisp",
        "--name",
        "parse-header",
        "--parent",
    ])
    .failure()
    .stderr(predicate::str::contains(
        "--parent moved above the top level",
    ));
}

// --- L7 stable ids ------------------------------------------------------

#[test]
fn a_stable_id_still_names_its_form_after_an_insertion_above_it() {
    let report = resolve(&["--query", "(format ...)", "--output", "json"])
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&report).expect("resolve emits json");
    let id = report["matches"][0]["id"]
        .as_str()
        .expect("a single-form match carries an id")
        .to_owned();

    let shifted = format!("(defvar *inserted* 1)\n{SOURCE}");
    paredit()
        .args(["edit", "select", "--dialect", "common-lisp", "--id", &id])
        .write_stdin(shifted)
        .assert()
        .success()
        .stdout("(format stream \"~a\" value)");
}

#[test]
fn an_id_that_no_longer_names_anything_says_so() {
    select(&["--dialect", "common-lisp", "--id", "0123456789abcdef"])
        .failure()
        .stderr(predicate::str::contains(
            "no form carries selector id 0123456789abcdef",
        ));
}

#[test]
fn a_malformed_id_is_refused_before_the_file_is_searched() {
    select(&["--dialect", "common-lisp", "--id", "not-an-id"])
        .failure()
        .stderr(predicate::str::contains(
            "selector id must be 16 lowercase hex characters",
        ));
}

// --- L8 `inspect resolve` -----------------------------------------------

#[test]
fn resolve_reports_every_match_with_coordinates_and_captures() {
    let output = resolve(&["--query", "(defun ?name ...)", "--output", "json"])
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).expect("json");

    assert_eq!(report["matchCount"], 2);
    assert_eq!(report["selector"], "--query '(defun ?name ...)'");
    let first = &report["matches"][0];
    assert_eq!(first["path"], "2");
    assert_eq!(first["kind"], "list");
    assert_eq!(first["head"], "defun");
    assert_eq!(first["start"]["line"], 4);
    assert_eq!(first["start"]["column"], 1);
    assert_eq!(first["captures"][0]["name"], "name");
    assert_eq!(first["captures"][0]["text"], "parse-header");
}

/// Reporting is the one place an ambiguous selector must *not* refuse: seeing
/// every match is how a caller decides whether to narrow it.
#[test]
fn resolve_never_refuses_an_ambiguous_selector() {
    let output = resolve(&["--query", "(cleanup ?x)", "--output", "json"])
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).expect("json");
    assert_eq!(report["matchCount"], 3);
}

#[test]
fn resolve_reports_no_match_as_a_result_unless_asked_to_fail() {
    resolve(&["--name", "absent", "--output", "json"])
        .success()
        .stdout(predicate::str::contains("\"matchCount\": 0"));
    resolve(&["--name", "absent", "--fail-on-empty"])
        .failure()
        .stderr(predicate::str::contains("no form matches --name absent"));
}

#[test]
fn resolve_text_output_is_one_tab_separated_line_per_match() {
    let output = resolve(&["--query", "(cleanup ?x)", "--output", "text"])
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).expect("utf-8");
    let mut lines = text.lines();
    assert_eq!(
        lines.next().expect("header"),
        "matches\t3\tselector\t--query '(cleanup ?x)'"
    );
    let first = lines.next().expect("first match");
    let columns = first.split('\t').collect::<Vec<_>>();
    // Top-level form 2 is `parse-header`; its child 3 is the `let`, whose
    // child 2 is the first `cleanup` (child 1 being the binding list).
    assert_eq!(columns[0], "2.3.2");
    assert_eq!(columns[2], "list");
    assert_eq!(columns[3].len(), 16, "column 4 is the stable id");
    assert_eq!(columns[4], "(cleanup line)");
}

// --- shared refusals ----------------------------------------------------

#[test]
fn no_selector_at_all_names_every_selector_that_exists() {
    select(&["--dialect", "common-lisp"])
        .failure()
        .stderr(predicate::str::contains(
            "no selector: pass one of --path, --at, --line-column, --name, --query, --id, \
             --select, or --from/--to",
        ));
}

#[test]
fn two_base_selectors_are_refused_by_the_argument_parser() {
    select(&["--dialect", "common-lisp", "--path", "0", "--name", "demo"])
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
}

// --- `--select` on the commands whose own flags claim the names ---------
//
// `refactor introduce-let --name` is the new binding's name and
// `refactor rename-binding --from` is a symbol, so those commands take one
// `--select` carrying a compact selector instead of the full flag surface.

#[test]
fn select_reaches_a_command_whose_name_flag_means_something_else() {
    let source = "(defun f (x)\n  (g (+ x 1) (+ x 1)))\n";
    paredit()
        .args([
            "refactor",
            "introduce-let",
            "--dialect",
            "common-lisp",
            // The selector; `--name` below is the *binding* being introduced.
            "--select",
            "query:(+ ?a ?b)",
            "--name",
            "sum",
            "--output",
            "json",
        ])
        .write_stdin(source)
        .assert()
        .failure()
        .stderr(predicate::str::contains("matches 2 forms"));

    paredit()
        .args([
            "refactor",
            "introduce-let",
            "--dialect",
            "common-lisp",
            "--select",
            "line:2:6",
            "--name",
            "sum",
            "--output",
            "json",
        ])
        .write_stdin(source)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"path\": \"0.3.1\""));
}

#[test]
fn select_accepts_a_bare_path_and_every_compact_prefix() {
    let source = "(defun f (x) (g x))";
    for selector in ["0.3", "path:0.3", "at:13", "line:1:14", "query:(g ?x)"] {
        paredit()
            .args([
                "refactor",
                "unwrap-call",
                "--dialect",
                "common-lisp",
                "--select",
                selector,
                "--output",
                "json",
            ])
            .write_stdin(source)
            .assert()
            .success();
    }
}

#[test]
fn an_unknown_compact_prefix_lists_the_ones_that_exist() {
    paredit()
        .args([
            "refactor",
            "unwrap-call",
            "--dialect",
            "common-lisp",
            "--select",
            "nonsense",
        ])
        .write_stdin("(defun f (x) (g x))")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "unknown selector prefix in `nonsense`",
        ));
}

/// A case-sensitive dialect must not fold the way Common Lisp does, or
/// `--query '(defn ?n ...)'` would match a Clojure `(Defn ...)`.
#[test]
fn a_query_respects_the_dialects_case_rules() {
    paredit()
        .args([
            "edit",
            "select",
            "--dialect",
            "clojure",
            "--query",
            "(defn ?n ...)",
        ])
        .write_stdin("(Defn a []) (defn b [])")
        .assert()
        .success()
        .stdout("(defn b [])");
}
