//! `inspect agent-report`: verbosity, token budgets, follow-ups, and deltas.

use super::*;

use std::path::Path;

const TWO_DEFINITIONS: &str = "(defun f (x)\n  (list x))\n(defun g (y) y)\n";

fn fixture(name: &str, source: &str) -> PathBuf {
    let dir = fresh_temp_dir(name);
    let path = dir.join("a.lisp");
    fs::write(&path, source).expect("write fixture");
    path
}

fn report(file: &Path, extra: &[&str]) -> serde_json::Value {
    let output = paredit()
        .args(["inspect", "agent-report", "--output", "json", "--file"])
        .arg(file)
        .args(extra)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).expect("agent-report emits valid JSON")
}

// --- H9: verbosity ---

/// The default has to stay what it was, or every existing consumer of this
/// report loses its lists to a flag it never passed.
#[test]
fn the_default_verbosity_still_carries_both_lists() {
    let file = fixture("agent-default", TWO_DEFINITIONS);
    let report = report(&file, &[]);

    assert_eq!(report["verbosity"], "normal");
    assert!(!report["outline"].as_array().expect("outline").is_empty());
    assert!(!report["atoms"].as_array().expect("atoms").is_empty());
}

/// Quiet drops the lists and keeps every count. That distinction is the point:
/// you still learn the file's shape and can decide whether to ask for detail.
#[test]
fn quiet_drops_the_lists_and_keeps_the_counts() {
    let file = fixture("agent-quiet", TWO_DEFINITIONS);
    let report = report(&file, &["--verbosity", "quiet"]);

    assert!(report["outline"].as_array().expect("outline").is_empty());
    assert!(report["atoms"].as_array().expect("atoms").is_empty());
    assert_eq!(report["metrics"]["outlineEntries"], 2);
    assert_eq!(report["metrics"]["definitionLikeForms"], 2);
    assert!(
        report["metrics"]["atomOccurrences"]
            .as_u64()
            .expect("atoms")
            > 0
    );
}

#[test]
fn detailed_adds_the_digest_and_the_distinct_atom_count() {
    let file = fixture("agent-detailed", TWO_DEFINITIONS);
    let report = report(&file, &["--verbosity", "detailed"]);

    assert!(
        report["digest"]
            .as_str()
            .expect("digest")
            .starts_with("fnv1a64:")
    );
    let distinct = report["distinctAtoms"].as_u64().expect("distinct");
    let total = report["metrics"]["atomOccurrences"]
        .as_u64()
        .expect("total");
    assert!(distinct <= total && distinct > 0);
}

// --- H8: token budget ---

/// A budget that is met changes nothing. `--max-tokens` must not reshape a
/// report it did not need to.
#[test]
fn a_budget_that_is_met_leaves_the_report_untouched() {
    let file = fixture("agent-budget-ample", TWO_DEFINITIONS);
    let unbounded = report(&file, &[]);
    let bounded = report(&file, &["--max-tokens", "100000"]);

    assert!(bounded["truncation"].is_null());
    assert_eq!(unbounded, bounded);
}

/// Silence is the failure mode. A trimmed list that does not say so reads as a
/// complete list.
#[test]
fn a_budget_that_bites_says_exactly_what_it_dropped() {
    let source = (0..200)
        .map(|index| format!("(defun name-{index} (a b c) (list a b c))\n"))
        .collect::<String>();
    let file = fixture("agent-budget-tight", &source);

    let report = report(&file, &["--max-tokens", "1500"]);
    let truncation = &report["truncation"];

    assert_eq!(truncation["truncated"], true);
    assert_eq!(truncation["budget_tokens"], 1500);

    let arrays = truncation["arrays"].as_array().expect("arrays");
    assert!(!arrays.is_empty());
    for array in arrays {
        let kept = array["kept"].as_u64().expect("kept");
        let total = array["total"].as_u64().expect("total");
        assert!(kept < total, "{array}");
        assert_eq!(array["dropped"].as_u64().expect("dropped"), total - kept);
    }

    // The counts survive truncation: they are how a caller learns what it is
    // missing and decides how to narrow the request.
    assert_eq!(report["metrics"]["outlineEntries"], 200);
}

/// Atoms are given up before the outline: an outline entry carries far more
/// per token, and a specific atom can be fetched with `inspect find-symbol`.
#[test]
fn atoms_are_given_up_before_the_outline() {
    let source = (0..200)
        .map(|index| format!("(defun name-{index} (a b c) (list a b c))\n"))
        .collect::<String>();
    let file = fixture("agent-budget-order", &source);

    let report = report(&file, &["--max-tokens", "3000"]);
    let atoms = report["atoms"].as_array().expect("atoms").len();
    let outline = report["outline"].as_array().expect("outline").len();
    assert!(atoms < outline, "atoms {atoms}, outline {outline}");
}

// --- H10: what to run next ---

#[test]
fn a_file_with_definitions_is_pointed_at_lint_and_complexity() {
    let file = fixture("agent-next", TWO_DEFINITIONS);
    let report = report(&file, &[]);

    let commands: Vec<&str> = report["next_commands"]
        .as_array()
        .expect("next_commands")
        .iter()
        .map(|entry| entry["command"].as_str().expect("command"))
        .collect();

    assert!(
        commands.iter().any(|c| c.contains("inspect lint")),
        "{commands:?}"
    );
    assert!(
        commands.iter().any(|c| c.contains("inspect complexity")),
        "{commands:?}"
    );
    // Runnable as written, naming this file.
    for command in &commands {
        assert!(command.starts_with("paredit "), "{command}");
        assert!(command.contains(&file.display().to_string()), "{command}");
    }
    // Every suggestion says what it would tell you.
    for entry in report["next_commands"].as_array().expect("next_commands") {
        assert!(!entry["why"].as_str().expect("why").is_empty());
    }
}

/// Reading from stdin there is no path to put in a command line, so the
/// file-shaped suggestions are withheld — a suggestion the caller has to
/// rewrite is not a suggestion. The one that needs no path still stands.
#[test]
fn reading_stdin_suggests_only_what_it_can_spell() {
    let output = paredit()
        .args(["inspect", "agent-report", "--output", "json"])
        .write_stdin(TWO_DEFINITIONS)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).expect("valid JSON");

    let commands: Vec<&str> = report["next_commands"]
        .as_array()
        .expect("next_commands")
        .iter()
        .map(|entry| entry["command"].as_str().expect("command"))
        .collect();

    // stdin has no extension, so the dialect is undetermined and that is worth
    // saying. Nothing else is, because nothing else can name the input.
    assert_eq!(commands, vec!["paredit config schema"]);
    assert!(!commands.iter().any(|command| command.contains("--file")));
}

// --- H7: the incremental report ---

#[test]
fn an_unchanged_file_reports_no_delta_at_all() {
    let file = fixture("agent-since-same", TWO_DEFINITIONS);
    let baseline = file.with_file_name("baseline.json");
    fs::write(
        &baseline,
        serde_json::to_string(&report(&file, &["--verbosity", "detailed"])).expect("serialize"),
    )
    .expect("write baseline");

    let report = report(&file, &["--since", &baseline.display().to_string()]);
    let delta = &report["delta"];

    assert_eq!(delta["unchanged"], true);
    assert!(
        delta["outline"]["added"]
            .as_array()
            .expect("added")
            .is_empty()
    );
    assert!(
        delta["outline"]["removed"]
            .as_array()
            .expect("removed")
            .is_empty()
    );
    assert!(
        delta["outline"]["moved"]
            .as_array()
            .expect("moved")
            .is_empty()
    );
}

/// The property a path-keyed delta gets wrong: inserting one definition above
/// two others is one addition and two moves, not three additions.
#[test]
fn inserting_a_definition_reports_one_addition_and_the_paths_that_shifted() {
    let file = fixture("agent-since-insert", TWO_DEFINITIONS);
    let baseline = file.with_file_name("baseline.json");
    fs::write(
        &baseline,
        serde_json::to_string(&report(&file, &[])).expect("serialize"),
    )
    .expect("write baseline");

    fs::write(&file, format!("(defun new (z) z)\n{TWO_DEFINITIONS}")).expect("rewrite");
    let report = report(&file, &["--since", &baseline.display().to_string()]);
    let delta = &report["delta"];

    assert_eq!(delta["unchanged"], false);

    let added = delta["outline"]["added"].as_array().expect("added");
    assert_eq!(added.len(), 1, "{added:?}");
    assert_eq!(added[0]["name"], "new");

    assert!(
        delta["outline"]["removed"]
            .as_array()
            .expect("removed")
            .is_empty()
    );

    let moved = delta["outline"]["moved"].as_array().expect("moved");
    assert_eq!(moved.len(), 2, "{moved:?}");
    assert_eq!(moved[0]["name"], "defun f");
    assert_eq!(moved[0]["from"], "0");
    assert_eq!(moved[0]["to"], "1");
}

#[test]
fn deleting_a_definition_is_reported_as_a_removal() {
    let file = fixture("agent-since-delete", TWO_DEFINITIONS);
    let baseline = file.with_file_name("baseline.json");
    fs::write(
        &baseline,
        serde_json::to_string(&report(&file, &[])).expect("serialize"),
    )
    .expect("write baseline");

    fs::write(&file, "(defun f (x)\n  (list x))\n").expect("rewrite");
    let report = report(&file, &["--since", &baseline.display().to_string()]);
    let removed = report["delta"]["outline"]["removed"]
        .as_array()
        .expect("removed");

    assert_eq!(removed.len(), 1);
    assert_eq!(removed[0]["name"], "g");
}

/// A baseline written at `--verbosity quiet` has an empty outline and full
/// counts, so a naive comparison would claim every definition was removed.
#[test]
fn a_baseline_with_no_outline_is_marked_as_not_comparable() {
    let file = fixture("agent-since-quiet", TWO_DEFINITIONS);
    let baseline = file.with_file_name("baseline.json");
    fs::write(
        &baseline,
        serde_json::to_string(&report(&file, &["--verbosity", "quiet"])).expect("serialize"),
    )
    .expect("write baseline");

    let report = report(&file, &["--since", &baseline.display().to_string()]);
    assert_eq!(report["delta"]["comparable"], false);
    assert_eq!(report["delta"]["unchanged"], false);
}

/// A file that happens to be JSON but is not this report would produce a delta
/// claiming everything changed — indistinguishable from a real answer.
#[test]
fn a_since_file_that_is_not_this_report_is_refused() {
    let file = fixture("agent-since-wrong", TWO_DEFINITIONS);
    let other = file.with_file_name("other.json");
    fs::write(&other, "{\"hello\": true}").expect("write");

    paredit()
        .args(["inspect", "agent-report", "--file"])
        .arg(&file)
        .args(["--since"])
        .arg(&other)
        .assert()
        .failure()
        .stderr(predicate::str::contains("is not an `inspect agent-report"));
}

#[test]
fn a_since_file_that_is_not_json_is_refused() {
    let file = fixture("agent-since-not-json", TWO_DEFINITIONS);
    let other = file.with_file_name("other.txt");
    fs::write(&other, "not json").expect("write");

    paredit()
        .args(["inspect", "agent-report", "--file"])
        .arg(&file)
        .args(["--since"])
        .arg(&other)
        .assert()
        .failure()
        .stderr(predicate::str::contains("is not JSON"));
}

/// The configuration supplies the default; the flag still wins.
#[test]
fn the_configuration_sets_the_default_verbosity_and_the_flag_beats_it() {
    let dir = fresh_temp_dir("agent-config-verbosity");
    fs::create_dir_all(dir.join(".git")).expect("git marker");
    fs::write(dir.join("a.lisp"), TWO_DEFINITIONS).expect("write source");
    fs::write(
        dir.join("paredit.toml"),
        "[output]\nverbosity = \"quiet\"\n",
    )
    .expect("write config");

    let configured = paredit()
        .current_dir(&dir)
        .args([
            "inspect",
            "agent-report",
            "--output",
            "json",
            "--file",
            "a.lisp",
        ])
        .env_remove("XDG_CONFIG_HOME")
        .env("HOME", dir.display().to_string())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let configured: serde_json::Value = serde_json::from_slice(&configured).expect("valid JSON");
    assert_eq!(configured["verbosity"], "quiet");

    let flagged = paredit()
        .current_dir(&dir)
        .args([
            "inspect",
            "agent-report",
            "--output",
            "json",
            "--verbosity",
            "detailed",
            "--file",
            "a.lisp",
        ])
        .env_remove("XDG_CONFIG_HOME")
        .env("HOME", dir.display().to_string())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let flagged: serde_json::Value = serde_json::from_slice(&flagged).expect("valid JSON");
    assert_eq!(flagged["verbosity"], "detailed");
}
