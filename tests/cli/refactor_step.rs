//! `refactor step`: taking a preview manifest one edit at a time.
//!
//! The domain's numbering and selector parsing are unit-tested next to
//! themselves. What these cover is the part that touches the filesystem — the
//! two hash guards, the parse check before a write, and the gate — because
//! every one of them is a way a partial apply could quietly corrupt a file.

use super::*;

const SOURCE: &str = "(defun helper (x) (* x 2))\n\
     (defun main (y)\n\
     \x20 (helper y)\n\
     \x20 (helper (helper y)))\n";

/// A directory holding a source file and the manifest that renames in it.
struct Prepared {
    file: PathBuf,
    manifest: PathBuf,
    hash: String,
}

fn prepare(name: &str) -> Prepared {
    let dir = fresh_temp_dir(name);
    let file = dir.join("core.lisp");
    fs::write(&file, SOURCE).expect("write lisp fixture");
    let manifest = dir.join("manifest.json");

    let assert = paredit()
        .args(["refactor", "preview", "--from", "helper", "--to", "helper2"])
        .arg("--manifest-out")
        .arg(&manifest)
        .arg(&file)
        .assert()
        .success();
    let summary: serde_json::Value =
        serde_json::from_slice(&assert.get_output().stdout).expect("preview summary");
    let hash = summary["manifest_hash"]
        .as_str()
        .expect("a manifest hash")
        .to_owned();

    Prepared {
        file,
        manifest,
        hash,
    }
}

fn step(prepared: &Prepared, extra: &[&str]) -> assert_cmd::assert::Assert {
    let mut command = paredit();
    command
        .args(["refactor", "step", "--output", "json"])
        .arg("--manifest")
        .arg(&prepared.manifest)
        .args(extra);
    command.assert()
}

fn json(assert: &assert_cmd::assert::Assert) -> serde_json::Value {
    serde_json::from_slice(&assert.get_output().stdout).expect("step report")
}

#[test]
fn cli_step_numbers_every_edit_and_writes_nothing_by_default() {
    let prepared = prepare("step-list");
    let report = json(&step(&prepared, &[]).success());

    assert_eq!(report["step_count"], 4);
    assert_eq!(report["selected"], 4);
    assert_eq!(report["write_requested"], false);
    assert_eq!(report["written"], serde_json::json!([]));
    // Numbered in source order, with the line and the replaced text.
    assert_eq!(report["steps"][0]["number"], 1);
    assert_eq!(report["steps"][0]["line"], 1);
    assert_eq!(report["steps"][0]["before"], "helper");
    assert_eq!(report["steps"][0]["after"], "helper2");
    assert_eq!(
        fs::read_to_string(&prepared.file).expect("read fixture"),
        SOURCE,
        "listing must not write"
    );
}

#[test]
fn cli_step_applies_only_the_accepted_steps() {
    let prepared = prepare("step-accept");
    let report = json(&step(&prepared, &["--accept", "1,2", "--write"]).success());

    assert_eq!(report["selected"], 2);
    assert_eq!(report["skipped"], 2);
    let written = fs::read_to_string(&prepared.file).expect("read fixture");
    assert!(written.starts_with("(defun helper2 (x)"), "{written}");
    // The two left out are still the old name.
    assert_eq!(written.matches("(helper ").count(), 2, "{written}");
}

#[test]
fn cli_step_skip_alone_means_everything_but_these() {
    let prepared = prepare("step-skip");
    let report = json(&step(&prepared, &["--skip", "1"]).success());
    assert_eq!(report["selected"], 3);
    assert_eq!(report["steps"][0]["selected"], false);
    assert_eq!(report["steps"][1]["selected"], true);
}

/// A manifest describes edits at byte offsets into the file as it was. Against
/// a file that has moved on, those offsets name different text — so the guard
/// has to fire before anything is listed, let alone written.
#[test]
fn cli_step_refuses_a_file_that_changed_since_the_manifest_was_written() {
    let prepared = prepare("step-stale-file");
    fs::write(&prepared.file, format!(";; a new line\n{SOURCE}")).expect("rewrite fixture");

    step(&prepared, &["--write"])
        .failure()
        .stderr(predicate::str::contains("has changed since the manifest"));
}

#[test]
fn cli_step_honours_the_manifest_hash_guard() {
    let prepared = prepare("step-hash");
    step(&prepared, &["--expect-manifest-hash", &prepared.hash]).success();
    step(
        &prepared,
        &["--expect-manifest-hash", "fnv1a64:0000000000000000"],
    )
    .failure()
    .stderr(predicate::str::contains("manifest hash mismatch"));
}

/// A step number the manifest does not have almost always means a stale plan,
/// and stepping the wrong plan is what this command exists to prevent.
#[test]
fn cli_step_refuses_a_selector_naming_a_step_that_does_not_exist() {
    let prepared = prepare("step-out-of-range");
    step(&prepared, &["--accept", "99"])
        .failure()
        .stderr(predicate::str::contains("stale"));
}

/// A manifest's edits are usually one change. A person may mean to split one;
/// a script should not be able to by accident.
#[test]
fn cli_step_fail_on_partial_gates_an_incomplete_selection() {
    let prepared = prepare("step-partial");
    step(&prepared, &["--accept", "1", "--fail-on-partial"])
        .code(3)
        .stderr(predicate::str::contains("one change"));

    step(&prepared, &["--accept", "all", "--fail-on-partial"]).success();
}

#[test]
fn cli_step_diff_previews_the_selection_without_writing() {
    let prepared = prepare("step-diff");
    paredit()
        .args(["refactor", "step", "--diff", "--accept", "1"])
        .arg("--manifest")
        .arg(&prepared.manifest)
        .assert()
        .success()
        .stdout(predicate::str::contains("+(defun helper2 (x)"))
        .stdout(predicate::str::contains("-(defun helper (x)"));

    assert_eq!(
        fs::read_to_string(&prepared.file).expect("read fixture"),
        SOURCE,
    );
}

/// The `git add -p` vocabulary, because a reviewer stepping a refactor already
/// knows it: `y` takes, `n` leaves, `a` takes the rest, `q` stops.
#[test]
fn cli_step_interactive_reads_one_decision_per_step() {
    let prepared = prepare("step-interactive");
    let assert = paredit()
        .args(["refactor", "step", "--interactive", "--output", "json"])
        .arg("--manifest")
        .arg(&prepared.manifest)
        .write_stdin("y\nn\na\n")
        .assert()
        .success();
    let report = json(&assert);

    assert_eq!(report["selected"], 3);
    assert_eq!(report["steps"][0]["selected"], true);
    assert_eq!(report["steps"][1]["selected"], false);
    // `a` took the third and everything after it.
    assert_eq!(report["steps"][2]["selected"], true);
    assert_eq!(report["steps"][3]["selected"], true);
}

/// Answering `q` stops, and stopping must not be read as accepting the rest.
#[test]
fn cli_step_interactive_quit_leaves_the_remaining_steps_alone() {
    let prepared = prepare("step-quit");
    let assert = paredit()
        .args(["refactor", "step", "--interactive", "--output", "json"])
        .arg("--manifest")
        .arg(&prepared.manifest)
        .write_stdin("y\nq\n")
        .assert()
        .success();
    assert_eq!(json(&assert)["selected"], 1);
}

/// Input ending is not consent either.
#[test]
fn cli_step_interactive_treats_a_closed_stream_as_no_further_answers() {
    let prepared = prepare("step-eof");
    let assert = paredit()
        .args(["refactor", "step", "--interactive", "--output", "json"])
        .arg("--manifest")
        .arg(&prepared.manifest)
        .write_stdin("y\n")
        .assert()
        .success();
    assert_eq!(json(&assert)["selected"], 1);
}
