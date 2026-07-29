//! The three run-wide controls: `--dry-run`, `--progress`, and the gate
//! taxonomy `inspect capabilities` publishes.

use super::*;

use std::path::Path;

const SOURCE: &str = "(defun f (x)\n  (list x))\n(defun g (y) y)\n";

fn fixture(name: &str) -> PathBuf {
    let dir = fresh_temp_dir(name);
    let path = dir.join("a.lisp");
    fs::write(&path, SOURCE).expect("write fixture");
    path
}

fn contents(path: &Path) -> String {
    fs::read_to_string(path).expect("read back")
}

// --- H13: --dry-run ---

/// The guarantee: appending `--dry-run` to a command line you did not
/// construct is enough to be certain nothing is written.
#[test]
fn dry_run_suppresses_write_and_leaves_the_file_alone() {
    let file = fixture("dry-run-write");
    let before = contents(&file);

    paredit()
        .args([
            "edit",
            "wrap",
            "--path",
            "0",
            "--write",
            "--dry-run",
            "--file",
        ])
        .arg(&file)
        .assert()
        .success();

    assert_eq!(contents(&file), before);
}

/// The rewritten document still goes to stdout, so `--dry-run` is a preview
/// rather than a refusal.
#[test]
fn dry_run_still_produces_the_result_on_stdout() {
    let file = fixture("dry-run-stdout");
    let output = paredit()
        .args([
            "edit",
            "wrap",
            "--path",
            "0",
            "--write",
            "--dry-run",
            "--file",
        ])
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let output = String::from_utf8(output).expect("UTF-8");

    assert!(output.starts_with("((defun f (x)"), "{output}");
    assert_eq!(contents(&file), SOURCE);
}

/// Never silent. The caller asked for a write and is not getting one.
#[test]
fn dry_run_says_that_it_suppressed_the_write() {
    let file = fixture("dry-run-note");
    paredit()
        .args([
            "edit",
            "wrap",
            "--path",
            "0",
            "--write",
            "--dry-run",
            "--file",
        ])
        .arg(&file)
        .assert()
        .success()
        .stderr(predicate::str::contains("--dry-run suppressed --write"));
}

/// Without a `--write` to suppress there is nothing to announce.
#[test]
fn dry_run_on_a_command_that_was_not_writing_says_nothing() {
    let file = fixture("dry-run-quiet");
    paredit()
        .args(["edit", "select", "--path", "0", "--dry-run", "--file"])
        .arg(&file)
        .assert()
        .success()
        .stderr(predicate::str::is_empty());
}

#[test]
fn the_environment_spelling_works_the_same_way() {
    let file = fixture("dry-run-env");
    let before = contents(&file);

    paredit()
        .args(["edit", "wrap", "--path", "0", "--write", "--file"])
        .arg(&file)
        .env("PAREDIT_DRY_RUN", "1")
        .assert()
        .success();

    assert_eq!(contents(&file), before);
}

/// After `--` a token is a file name, and one that happens to be spelled
/// `--write` is still a file name.
#[test]
fn a_write_after_a_double_dash_is_not_suppressed_as_a_flag() {
    let file = fixture("dry-run-separator");
    let output = paredit()
        .args(["inspect", "lint", "--dry-run", "--"])
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    // The point is that the positional survived. Had the separator handling
    // eaten it, `clap` would have refused the command for a missing argument
    // rather than producing a report at all.
    let report: serde_json::Value = serde_json::from_slice(&output).expect("valid JSON");
    assert!(report["finding_count"].is_number(), "{report}");
}

/// It reaches every namespace, not only `edit`.
#[test]
fn dry_run_reaches_a_refactor_command_too() {
    let file = fixture("dry-run-refactor");
    let before = contents(&file);

    paredit()
        .args([
            "refactor",
            "rename-symbols",
            "--from",
            "f",
            "--to",
            "h",
            "--write",
        ])
        .arg(&file)
        .args(["--dry-run"])
        .assert()
        .success();

    assert_eq!(contents(&file), before);
}

/// Documented, not hidden. A control a caller cannot discover is a trap.
#[test]
fn dry_run_and_progress_are_documented_on_every_command() {
    let map = capability_map();
    for command in ["inspect lint", "edit wrap", "refactor plan", "config check"] {
        let flags = map
            .get(command)
            .unwrap_or_else(|| panic!("{command} is not in the capability map"));
        assert!(
            flags.contains("dry-run"),
            "{command} does not document --dry-run"
        );
        assert!(
            flags.contains("progress"),
            "{command} does not document --progress"
        );
    }
}

// --- H15: JSON Lines progress ---

#[test]
fn progress_emits_one_json_object_per_line_on_stderr() {
    let dir = fresh_temp_dir("progress-lines");
    for name in ["a", "b", "c"] {
        fs::write(dir.join(format!("{name}.lisp")), SOURCE).expect("write");
    }

    let output = paredit()
        .args(["inspect", "lint", "--progress", "--output", "json"])
        .arg(&dir)
        .assert()
        .success()
        .get_output()
        .clone();

    let stderr = String::from_utf8(output.stderr).expect("UTF-8");
    let events: Vec<serde_json::Value> = stderr
        .lines()
        .map(|line| serde_json::from_str(line).unwrap_or_else(|error| panic!("{error}: {line:?}")))
        .collect();

    assert!(!events.is_empty(), "no progress was emitted");
    let discovered = events
        .iter()
        .find(|event| event["event"] == "discovered")
        .expect("a discovery event");
    assert_eq!(discovered["files"], 3);

    let files: Vec<&serde_json::Value> = events
        .iter()
        .filter(|event| event["event"] == "file")
        .collect();
    assert_eq!(files.len(), 3);
    // Sequence numbers count up, so a consumer can tell it missed a line.
    for (index, event) in files.iter().enumerate() {
        assert_eq!(event["sequence"], index as u64 + 1);
    }

    // The report itself must be untouched on stdout: a progress line landing
    // in the middle of the JSON document would break every consumer of it.
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout is still one JSON document");
    assert!(report["finding_count"].is_number(), "{report}");
}

#[test]
fn without_the_flag_stderr_stays_empty() {
    let file = fixture("progress-off");
    paredit()
        .args(["inspect", "lint", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stderr(predicate::str::is_empty());
}

/// Progress must not change the report. An observability switch that alters
/// the answer is not observability.
#[test]
fn progress_does_not_change_stdout() {
    let file = fixture("progress-identical");
    let without = paredit()
        .args(["inspect", "lint", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let with = paredit()
        .args(["inspect", "lint", "--progress", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert_eq!(
        String::from_utf8_lossy(&without),
        String::from_utf8_lossy(&with)
    );
}

// --- H16: the gate taxonomy ---

fn capabilities() -> serde_json::Value {
    let output = paredit()
        .args(["inspect", "capabilities"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).expect("valid JSON")
}

fn every_command(report: &serde_json::Value) -> Vec<(String, serde_json::Value)> {
    fn walk(
        commands: &[serde_json::Value],
        prefix: &str,
        found: &mut Vec<(String, serde_json::Value)>,
    ) {
        for command in commands {
            let name = command["name"].as_str().expect("name");
            let path = if prefix.is_empty() {
                name.to_owned()
            } else {
                format!("{prefix} {name}")
            };
            match command["commands"].as_array() {
                Some(children) => walk(children, &path, found),
                None => found.push((path, command.clone())),
            }
        }
    }
    let mut found = Vec::new();
    walk(
        report["commands"].as_array().expect("commands"),
        "",
        &mut found,
    );
    found
}

/// The point of publishing gates: an agent can ask "how do I make this
/// command fail on its findings" without reading 275 help texts.
#[test]
fn capabilities_publishes_the_gate_flags_of_each_command() {
    let report = capabilities();
    let commands = every_command(&report);

    let lint = commands
        .iter()
        .find(|(path, _)| path == "inspect lint")
        .expect("inspect lint");
    let gates: Vec<(&str, &str)> = lint.1["gates"]
        .as_array()
        .expect("gates")
        .iter()
        .map(|gate| {
            (
                gate["flag"].as_str().expect("flag"),
                gate["kind"].as_str().expect("kind"),
            )
        })
        .collect();

    assert!(gates.contains(&("--fail-on", "severity")), "{gates:?}");
    assert!(
        gates.contains(&("--fail-on-finding", "presence")),
        "{gates:?}"
    );
}

/// The convention, enforced. Every gate must be one of the three spellings,
/// must name the exit status it produces, and must explain itself.
#[test]
fn every_published_gate_follows_the_convention() {
    let report = capabilities();
    let mut total = 0;

    for (path, command) in every_command(&report) {
        let Some(gates) = command["gates"].as_array() else {
            continue;
        };
        assert!(!gates.is_empty(), "{path} published an empty gate list");

        for gate in gates {
            total += 1;
            let flag = gate["flag"].as_str().expect("flag");
            let kind = gate["kind"].as_str().expect("kind");

            assert_eq!(gate["exit_code"], 3, "{path} {flag}");
            assert!(
                gate["help"].as_str().is_some_and(|help| !help.is_empty()),
                "{path} {flag} has no help"
            );

            match kind {
                "severity" => assert_eq!(flag, "--fail-on", "{path}"),
                "presence" => assert!(flag.starts_with("--fail-on-"), "{path} {flag}"),
                "minimum" => assert!(flag.starts_with("--require-"), "{path} {flag}"),
                other => panic!("{path} {flag} has unknown gate kind {other}"),
            }
        }
    }

    // A run that classified nothing would pass every assertion above.
    assert!(total > 100, "only {total} gates found; the walk is wrong");
}

/// Absent rather than empty, so "cannot fail on a policy" and "we did not
/// look" stay distinguishable.
#[test]
fn a_command_with_no_gate_has_no_gates_field() {
    let report = capabilities();
    let commands = every_command(&report);
    let select = commands
        .iter()
        .find(|(path, _)| path == "edit select")
        .expect("edit select");
    assert!(select.1["gates"].is_null(), "{}", select.1);
}

// --- H18: the message language ---

/// The scope, checked: what a person reads is translated, what a program
/// matches on is not.
#[test]
fn japanese_translates_the_diagnostic_and_leaves_the_identifiers_alone() {
    let file = fixture("language-ja");
    let stderr = paredit()
        .args(["edit", "select", "--path", "0.9", "--file"])
        .arg(&file)
        .env("PAREDIT_OUTPUT_LANGUAGE", "ja")
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    let text = String::from_utf8(stderr).expect("UTF-8");

    assert!(text.starts_with("エラー ["), "{text}");
    assert!(text.contains("対処: "), "{text}");
    // The code is an identifier a consumer matches on, so it stays.
    assert!(text.contains("selection.path-not-reachable"), "{text}");
    // And a suggested command has to remain runnable.
    assert!(text.contains("paredit inspect outline"), "{text}");
    assert!(text.contains("--at"), "{text}");
}

#[test]
fn the_json_error_keeps_english_identifiers_and_translates_the_description() {
    let file = fixture("language-ja-json");
    let stderr = paredit()
        .args(["inspect", "form", "--path", "0.9", "--file"])
        .arg(&file)
        .env("PAREDIT_OUTPUT_LANGUAGE", "ja")
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&stderr).expect("stderr is JSON");

    assert_eq!(report["error"]["code"], "selection.path-not-reachable");
    assert_eq!(report["error"]["category"], "selection");
    assert_eq!(
        report["error"]["category_description"],
        "選択が解決できませんでした"
    );
    assert!(
        report["error"]["repairs"][0]["action"]
            .as_str()
            .expect("action")
            .is_ascii(),
        "the action is an identifier and must stay ASCII"
    );
}

/// A report payload is not a diagnostic: translating a finding's `kind` would
/// break every consumer to help nobody.
#[test]
fn a_report_payload_stays_english_whatever_the_language() {
    let dir = fresh_temp_dir("language-payload");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (x) (if (eq x nil) 1 2))\n").expect("write");

    let english = paredit()
        .args(["inspect", "lint", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let japanese = paredit()
        .args(["inspect", "lint", "--output", "json"])
        .arg(&file)
        .env("PAREDIT_OUTPUT_LANGUAGE", "ja")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    assert_eq!(
        String::from_utf8_lossy(&english),
        String::from_utf8_lossy(&japanese)
    );
}

#[test]
fn english_is_the_default() {
    let file = fixture("language-default");
    paredit()
        .args(["edit", "select", "--path", "0.9", "--file"])
        .arg(&file)
        .assert()
        .failure()
        .stderr(predicate::str::starts_with("Error ["));
}
