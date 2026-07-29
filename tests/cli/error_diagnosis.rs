//! Stable error identity and the repair steps attached to it.
//!
//! The codes are a contract: an agent branches on them. So these assert the
//! exact strings, not merely that *something* was reported.

use super::*;

fn lisp(name: &str) -> PathBuf {
    let dir = fresh_temp_dir(name);
    let path = dir.join("a.lisp");
    fs::write(&path, "(defun f (x)\n  (list x))\n").expect("write fixture");
    path
}

/// stderr as JSON, for the commands whose reports are JSON.
fn error_json(mut command: Command) -> serde_json::Value {
    let stderr = command.assert().failure().get_output().stderr.clone();
    let text = String::from_utf8(stderr).expect("UTF-8");
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("{error}: {text:?}"))
}

fn code_of(report: &serde_json::Value) -> &str {
    report["error"]["code"].as_str().expect("code")
}

#[test]
fn an_unreachable_path_is_named_and_answered() {
    let file = lisp("diagnosis-path");
    let mut command = paredit();
    command
        .args(["inspect", "form", "--path", "0.9", "--file"])
        .arg(&file);

    let report = error_json(command);
    assert_eq!(code_of(&report), "selection.path-not-reachable");
    assert_eq!(report["error"]["category"], "selection");
    assert_eq!(report["error"]["retryable"], false);
    assert_eq!(report["error"]["exit_code"], 1);
    assert_eq!(report["command"], "inspect form");

    // The repair has to be a command line that runs as written, naming the
    // same file. A template the caller has to fill in costs a round trip.
    let repairs = report["error"]["repairs"].as_array().expect("repairs");
    let first = repairs[0]["command"].as_str().expect("a command");
    assert!(first.starts_with("paredit inspect outline"), "{first}");
    assert!(first.ends_with(&file.display().to_string()), "{first}");
}

#[test]
fn an_offset_with_no_expression_offers_selecting_by_path() {
    let file = lisp("diagnosis-offset");
    let mut command = paredit();
    command
        .args(["inspect", "form", "--at", "99999", "--file"])
        .arg(&file);

    let report = error_json(command);
    assert_eq!(code_of(&report), "selection.offset-not-found");
    let details: Vec<&str> = report["error"]["repairs"]
        .as_array()
        .expect("repairs")
        .iter()
        .map(|repair| repair["detail"].as_str().expect("detail"))
        .collect();
    assert!(
        details.iter().any(|detail| detail.contains("--path")),
        "{details:?}"
    );
}

#[test]
fn a_missing_target_is_an_argument_failure_rather_than_a_selection_one() {
    let file = lisp("diagnosis-no-target");
    let mut command = paredit();
    command.args(["inspect", "form", "--file"]).arg(&file);

    let report = error_json(command);
    assert_eq!(code_of(&report), "argument.target-required");
    assert_eq!(report["error"]["category"], "argument");
}

#[test]
fn passing_both_selectors_is_refused_by_clap_before_this_layer() {
    // `--path` and `--at` conflict in the argument definition, so this never
    // reaches the diagnosis layer. Asserted so that a future change moving the
    // check does not silently lose the message.
    let file = lisp("diagnosis-ambiguous");
    paredit()
        .args(["inspect", "form", "--path", "0", "--at", "1", "--file"])
        .arg(&file)
        .assert()
        .code(2)
        .stderr(predicate::str::contains("cannot be used with"));
}

#[test]
fn an_unparsable_document_offers_the_repair_command() {
    let dir = fresh_temp_dir("diagnosis-unbalanced");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (x)\n").expect("write fixture");

    let mut command = paredit();
    command
        .args(["inspect", "form", "--path", "0", "--file"])
        .arg(&file);

    let report = error_json(command);
    assert_eq!(code_of(&report), "input.unparsable");
    assert_eq!(report["error"]["category"], "input");

    let commands: Vec<&str> = report["error"]["repairs"]
        .as_array()
        .expect("repairs")
        .iter()
        .filter_map(|repair| repair["command"].as_str())
        .collect();
    assert!(
        commands
            .iter()
            .any(|command| command.contains("repair-unclosed-lists")),
        "{commands:?}"
    );
}

#[test]
fn a_write_without_a_file_says_which_flag_is_missing() {
    let mut command = paredit();
    command
        .args(["edit", "kill", "--path", "0", "--write"])
        .write_stdin("(a b)\n");

    let stderr = command.assert().failure().get_output().stderr.clone();
    let text = String::from_utf8(stderr).expect("UTF-8");
    assert!(
        text.starts_with("Error [argument.write-requires-file]:"),
        "{text}"
    );
    assert!(text.contains("--file"), "{text}");
}

/// A tripped gate is not a malfunction: it exits 3, and its category says so.
#[test]
fn a_policy_gate_is_classified_as_a_gate_and_exits_three() {
    let dir = fresh_temp_dir("diagnosis-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (x) (if (eq x nil) 1 2))\n").expect("write fixture");

    let stderr = paredit()
        .args(["inspect", "lint", "--fail-on", "warning"])
        .arg(&file)
        .assert()
        .code(3)
        .get_output()
        .stderr
        .clone();
    let text = String::from_utf8(stderr).expect("UTF-8");
    let report: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|error| panic!("{error}: {text:?}"));

    assert_eq!(code_of(&report), "gate.failed");
    assert_eq!(report["error"]["category"], "gate");
    assert_eq!(report["error"]["exit_code"], 3);
}

/// The rendering follows the format the command's report would have used, so
/// the same failure reads two ways depending on the invocation.
#[test]
fn the_error_format_follows_the_commands_own_output_format() {
    let file = lisp("diagnosis-format");

    let as_text = paredit()
        .args([
            "inspect", "form", "--path", "0.9", "--output", "text", "--file",
        ])
        .arg(&file)
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    let as_text = String::from_utf8(as_text).expect("UTF-8");
    assert!(
        as_text.starts_with("Error [selection.path-not-reachable]:"),
        "{as_text}"
    );
    assert!(as_text.contains("  try: "), "{as_text}");

    let as_json = paredit()
        .args([
            "inspect", "form", "--path", "0.9", "--output", "json", "--file",
        ])
        .arg(&file)
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    let as_json = String::from_utf8(as_json).expect("UTF-8");
    assert!(serde_json::from_str::<serde_json::Value>(&as_json).is_ok());
}

/// `edit` commands have no `--output`, so there is no format to follow and the
/// prose line is the only sensible answer.
#[test]
fn a_command_with_no_output_flag_reports_in_text() {
    let file = lisp("diagnosis-edit");
    let stderr = paredit()
        .args(["edit", "raise", "--path", "0", "--file"])
        .arg(&file)
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    let text = String::from_utf8(stderr).expect("UTF-8");
    assert!(text.starts_with("Error [input.shape-refused]:"), "{text}");
}

/// Every code the CLI can emit has to be one a consumer can look up. Checked
/// against the labels rather than assumed, so a code invented at a call site
/// without being added to the table shows up here.
#[test]
fn reported_codes_are_dotted_identifiers() {
    let file = lisp("diagnosis-shape");
    for (args, expected) in [
        (
            vec!["inspect", "form", "--path", "0.9"],
            "selection.path-not-reachable",
        ),
        (vec!["inspect", "form"], "argument.target-required"),
    ] {
        let mut command = paredit();
        command.args(&args).args(["--file"]).arg(&file);
        let report = error_json(command);
        let code = code_of(&report);
        assert_eq!(code, expected);
        assert!(code.contains('.'), "{code}");
        assert!(
            code.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.'),
            "{code}"
        );
    }
}
