//! The reports that measure a tree rather than judge one form in it.
//!
//! Unlike the semantic and reader reports, these are dialect-neutral: line
//! shape, comments, and docstring position are the same question in every
//! dialect this build parses, so every one of them answers rather than going
//! silent.

use super::*;

const FIXTURE: &str = "(defun render-pane (limit)\n\
     \x20 \"Stops after COUNT items.\"\n\
     \x20 ;; TODO(ada): cache the pane\n\
     \x20 (dotimes (i limit) i))\n\
     (defun undocumented (x y)\n\
     \x20 ;; FIXME: this is wrong\n\
     \x20 (list x y))\n";

fn fixture(name: &str) -> PathBuf {
    let dir = fresh_temp_dir(name);
    let file = dir.join("core.lisp");
    fs::write(&file, FIXTURE).expect("write lisp fixture");
    file
}

const COMMANDS: [&str; 8] = [
    "docstrings",
    "todo",
    "line-metrics",
    "indentation",
    "duplication-ratio",
    "cohesion",
    "hotspots",
    "debt-score",
];

#[test]
fn cli_docstrings_reports_a_parameter_the_lambda_list_does_not_have() {
    paredit()
        .args(["inspect", "docstrings", "--output", "json"])
        .arg(fixture("inspect-docstrings"))
        .assert()
        .success()
        .stdout(predicate::str::contains("\"issue\": \"stale-parameter\""))
        .stdout(predicate::str::contains("\"COUNT\""))
        .stdout(predicate::str::contains("\"issue\": \"missing\""));
}

#[test]
fn cli_docstrings_fail_on_defect_trips_gate() {
    paredit()
        .args(["inspect", "docstrings", "--fail-on-defect"])
        .arg(fixture("inspect-docstrings-gate"))
        .assert()
        .code(3)
        .stderr(predicate::str::contains("inspect docstrings policy failed"));
}

#[test]
fn cli_docstrings_passes_the_gate_for_a_consistent_docstring() {
    let dir = fresh_temp_dir("inspect-docstrings-clean");
    let file = dir.join("core.lisp");
    fs::write(&file, "(defun f (x) \"Returns X unchanged.\" x)\n").expect("write lisp fixture");

    paredit()
        .args([
            "inspect",
            "docstrings",
            "--fail-on-defect",
            "--output",
            "json",
        ])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"defect_count\": 0"));
}

#[test]
fn cli_todo_names_the_definition_and_the_author() {
    paredit()
        .args(["inspect", "todo", "--output", "json"])
        .arg(fixture("inspect-todo"))
        .assert()
        .success()
        .stdout(predicate::str::contains("\"marker\": \"TODO\""))
        .stdout(predicate::str::contains("\"author\": \"ada\""))
        .stdout(predicate::str::contains("\"definition\": \"render-pane\""))
        .stdout(predicate::str::contains("\"urgent_count\": 1"));
}

#[test]
fn cli_line_metrics_reports_nothing_under_the_default_thresholds() {
    paredit()
        .args(["inspect", "line-metrics", "--output", "json"])
        .arg(fixture("inspect-line-metrics"))
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        .stdout(predicate::str::contains("\"definition_count\": 2"));
}

#[test]
fn cli_line_metrics_thresholds_are_configurable() {
    paredit()
        .args([
            "inspect",
            "line-metrics",
            "--max-definition-lines",
            "1",
            "--output",
            "json",
        ])
        .arg(fixture("inspect-line-metrics-tight"))
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"overflow\": \"long-definition\"",
        ))
        .stdout(predicate::str::contains("\"threshold\": 1"));
}

#[test]
fn cli_line_metrics_fail_on_overflow_trips_gate() {
    paredit()
        .args([
            "inspect",
            "line-metrics",
            "--max-line-length",
            "5",
            "--fail-on-overflow",
        ])
        .arg(fixture("inspect-line-metrics-gate"))
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "inspect line-metrics policy failed",
        ));
}

/// These measure shape rather than semantics, so — unlike the reports in
/// `semantic_report` and `lisp_analysis_report` — every dialect is measured
/// rather than labelled as unmodelled.
#[test]
fn cli_indentation_reports_a_body_indented_against_the_convention() {
    let dir = fresh_temp_dir("inspect-indentation");
    let file = dir.join("core.lisp");
    fs::write(&file, "(defun f (x)\n    (list x))\n").expect("write lisp fixture");

    paredit()
        .args(["inspect", "indentation", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"kind\": \"body-indent\""))
        .stdout(predicate::str::contains("\"actual_column\": 4"))
        .stdout(predicate::str::contains("\"expected_column\": 2"));
}

#[test]
fn cli_indentation_accepts_the_conventional_two_spaces() {
    let dir = fresh_temp_dir("inspect-indentation-clean");
    let file = dir.join("core.lisp");
    fs::write(&file, "(defun f (x)\n  (list x))\n").expect("write lisp fixture");

    paredit()
        .args([
            "inspect",
            "indentation",
            "--fail-on-deviation",
            "--output",
            "json",
        ])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_duplication_ratio_reports_a_repeated_shape_and_the_ratio() {
    let dir = fresh_temp_dir("inspect-duplication-ratio");
    let file = dir.join("core.lisp");
    fs::write(&file, "(defun f (a) (list (g a 1 2) (g a 1 2)))\n").expect("write lisp fixture");

    paredit()
        .args(["inspect", "duplication-ratio", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"kind\": \"repeated-shape\""))
        .stdout(predicate::str::contains("\"occurrences\": 2"))
        .stdout(predicate::str::contains("duplication_per_mille"));
}

#[test]
fn cli_cohesion_reports_an_isolated_definition() {
    let dir = fresh_temp_dir("inspect-cohesion");
    let file = dir.join("core.lisp");
    fs::write(
        &file,
        "(in-package :app)\n(defun a () (b))\n(defun b () 1)\n(defun c () (elsewhere))\n",
    )
    .expect("write lisp fixture");

    paredit()
        .args(["inspect", "cohesion", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"kind\": \"isolated\""))
        .stdout(predicate::str::contains("\"package\": \"APP\""))
        .stdout(predicate::str::contains("cohesion_per_mille"));
}

/// The fixture lives in a temp directory, which is not a git repository, so
/// this also pins the fallback: churn is unavailable and the report says so
/// instead of reporting a zero that reads like "this file never changes".
#[test]
fn cli_hotspots_says_so_when_git_cannot_answer() {
    paredit()
        .args(["inspect", "hotspots", "--output", "json"])
        .arg(fixture("inspect-hotspots"))
        .assert()
        .success()
        .stdout(predicate::str::contains("churn_unavailable"))
        .stdout(predicate::str::contains("\"kind\": \"hotspot\""));
}

#[test]
fn cli_debt_score_shows_the_contribution_of_every_input() {
    paredit()
        .args(["inspect", "debt-score", "--output", "json"])
        .arg(fixture("inspect-debt-score"))
        .assert()
        .success()
        .stdout(predicate::str::contains("\"contributions\""))
        .stdout(predicate::str::contains("missing-docstring="))
        .stdout(predicate::str::contains("parked-work="));
}

#[test]
fn cli_every_code_metrics_report_answers_for_a_non_common_lisp_dialect() {
    let dir = fresh_temp_dir("inspect-code-metrics-dialect");
    let file = dir.join("core.clj");
    fs::write(&file, ";; TODO: port this\n(defn f [x] x)\n").expect("write clojure fixture");

    for command in COMMANDS {
        paredit()
            .args(["inspect", command, "--output", "json"])
            .arg(&file)
            .assert()
            .success()
            .stdout(predicate::str::contains("\"dialect_modelled\": true"));
    }
}

#[test]
fn cli_every_code_metrics_report_is_byte_identical_across_runs() {
    let file = fixture("inspect-code-metrics-determinism");

    for command in COMMANDS {
        let run = || {
            paredit()
                .args(["inspect", command, "--output", "json"])
                .arg(&file)
                .assert()
                .success()
                .get_output()
                .stdout
                .clone()
        };
        assert_eq!(run(), run(), "{command} is not deterministic");
    }
}
