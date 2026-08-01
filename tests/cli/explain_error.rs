//! `--explain-error`: the `doc_url` section body, expanded inline.
//!
//! FR-001. Runs through the real binary because the thing being checked is
//! the wiring — the flag reaching `report_failure` and the embedded markdown
//! actually being sliced correctly — not the extraction logic itself, which
//! `packages/core/cli/src/diagnosis.rs` already covers as unit tests.

use super::*;

const SOURCE: &str = "(defun f (x)\n  (list x))\n";

fn unreachable_path(extra: &[&str]) -> assert_cmd::assert::Assert {
    let mut command = paredit();
    command.args(["inspect", "form", "--path", "0.9"]);
    command.args(extra);
    command.write_stdin(SOURCE).assert()
}

#[test]
fn without_the_flag_the_json_explanation_key_is_null() {
    let output = unreachable_path(&["--output", "json"])
        .failure()
        .get_output()
        .stderr
        .clone();
    let report: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output).expect("utf-8")).expect("json");
    assert!(report["error"]["explanation"].is_null());
}

#[test]
fn the_flag_adds_the_sections_prose_to_the_json_envelope() {
    let mut command = paredit();
    command.args([
        "--explain-error",
        "inspect",
        "form",
        "--path",
        "0.9",
        "--output",
        "json",
    ]);
    let output = command
        .write_stdin(SOURCE)
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    let report: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output).expect("utf-8")).expect("json");
    assert_eq!(report["error"]["code"], "selection.path-not-reachable");
    let explanation = report["error"]["explanation"]
        .as_str()
        .expect("explanation");
    // Exact text of `docs/src/reference/errors.md`'s
    // `selection.path-not-reachable` section — this assertion breaks, on
    // purpose, if that prose and this flag's output ever drift apart.
    assert!(
        explanation.contains("does not exist in this document's tree"),
        "{explanation:?}"
    );
    // The doc_url the explanation supposedly expands should name the same
    // code, so the two cannot silently point at different sections.
    assert!(
        report["error"]["doc_url"]
            .as_str()
            .expect("doc_url")
            .ends_with("#selection.path-not-reachable")
    );
}

#[test]
fn the_flag_adds_the_explanation_after_the_repairs_in_text_mode() {
    let mut command = paredit();
    command.args(["--explain-error", "inspect", "form", "--path", "0.9"]);
    let output = command
        .write_stdin(SOURCE)
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    let text = String::from_utf8(output).expect("utf-8");
    assert!(
        text.contains("does not exist in this document's tree"),
        "{text:?}"
    );
}

#[test]
fn without_the_flag_text_mode_is_unchanged() {
    let output = unreachable_path(&[]).failure().get_output().stderr.clone();
    let text = String::from_utf8(output).expect("utf-8");
    assert!(
        !text.contains("does not exist in this document's tree"),
        "{text:?}"
    );
}
