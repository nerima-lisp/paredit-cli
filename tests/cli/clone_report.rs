use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::Value;

use super::{fresh_temp_dir, paredit};

fn json_from(args: &[&str]) -> Value {
    let output = paredit().args(args).output().expect("run paredit");
    assert!(
        output.status.success(),
        "command failed: {}\n{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("valid JSON")
}

fn write(dir: &Path, name: &str, contents: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create fixture directory");
    }
    fs::write(&path, contents).expect("write fixture");
    path
}

// ------------------------------------------------------------- clone-classes

#[test]
fn clone_classes_groups_a_renamed_pair_and_labels_it_type_2() {
    let dir = fresh_temp_dir("clone-classes-json");
    write(
        &dir,
        "a.lisp",
        "(defun alpha (x) (+ x 1))\n(defun beta (y) (+ y 2))\n",
    );

    let report = json_from(&[
        "inspect",
        "clone-classes",
        "--threshold",
        "0.8",
        "--form-scope",
        "top-level",
        dir.to_str().expect("utf-8 path"),
    ]);

    assert_eq!(report["schema_version"], 1);
    let classes = report["classes"].as_array().expect("classes");
    assert_eq!(
        report["class_count"].as_u64().expect("count") as usize,
        classes.len()
    );
    assert_eq!(classes.len(), 1);
    assert_eq!(classes[0]["clone_type"], "type-2");
    assert_eq!(classes[0]["clone_type_number"], 2);
    assert_eq!(classes[0]["consistent_renaming"], true);
    assert_eq!(classes[0]["member_count"], 2);
    assert_eq!(classes[0]["rank"], 1);
    assert!(
        classes[0]["extraction"]["total_lines"]
            .as_u64()
            .expect("lines")
            >= 2
    );
}

#[test]
fn clone_classes_ranks_the_biggest_saving_first() {
    let dir = fresh_temp_dir("clone-classes-rank");
    write(
        &dir,
        "a.lisp",
        "\
(defun short-a (x) (+ x 1))
(defun short-b (y) (+ y 2))
(defun long-a (x)
  (frobnicate x)
  (frobnicate x)
  (frobnicate x)
  (frobnicate x))
(defun long-b (y)
  (frobnicate y)
  (frobnicate y)
  (frobnicate y)
  (frobnicate y))
",
    );

    let report = json_from(&[
        "inspect",
        "clone-classes",
        "--threshold",
        "0.8",
        "--form-scope",
        "top-level",
        dir.to_str().expect("utf-8 path"),
    ]);

    let classes = report["classes"].as_array().expect("classes");
    assert_eq!(classes.len(), 2);
    let saved = classes
        .iter()
        .map(|class| class["extraction"]["saved_lines"].as_u64().expect("saved"))
        .collect::<Vec<_>>();
    assert!(saved[0] >= saved[1], "{saved:?}");
    assert_eq!(classes[0]["rank"], 1);
    assert_eq!(classes[1]["rank"], 2);
}

#[test]
fn clone_classes_reports_one_class_for_nested_echoes_of_one_duplicate() {
    // A duplicated `defun` contains a duplicated `let` containing a duplicated
    // `dolist`. There is one thing to extract, not three, and the default
    // overlap policy is what says so.
    let dir = fresh_temp_dir("clone-classes-overlap");
    write(
        &dir,
        "a.lisp",
        "\
(defun alpha (x)
  (let ((total 0))
    (dolist (item x) (incf total item))
    total))
(defun beta (y)
  (let ((sum 0))
    (dolist (elem y) (incf sum elem))
    sum))
",
    );
    let root = dir.to_str().expect("utf-8 path");

    let maximal = json_from(&["inspect", "clone-classes", root]);
    assert_eq!(maximal["class_count"], 1);
    assert_eq!(maximal["classes"][0]["member_count"], 2);
    assert_eq!(maximal["classes"][0]["members"][0]["head"], "defun");

    // `--overlap-policy all` opts back into the echoes.
    let all = json_from(&["inspect", "clone-classes", "--overlap-policy", "all", root]);
    assert!(
        all["class_count"].as_u64().expect("count") > 1,
        "{}",
        all["class_count"]
    );
}

#[test]
fn clone_classes_filters_by_clone_type_and_reports_what_it_dropped() {
    let dir = fresh_temp_dir("clone-classes-filter");
    write(
        &dir,
        "a.lisp",
        "\
(defun alpha (x) (+ x 1))
(defun beta (y) (+ y 2))
(defun same (a b c) (list a b c))
(defun same (a b c) (list a b c))
",
    );
    let args = [
        "inspect",
        "clone-classes",
        "--threshold",
        "0.8",
        "--form-scope",
        "top-level",
        dir.to_str().expect("utf-8 path"),
    ];

    let all = json_from(&args);
    assert_eq!(all["class_count"], 2);

    let mut filtered_args = args.to_vec();
    filtered_args.extend(["--clone-type", "1"]);
    let filtered = json_from(&filtered_args);
    assert_eq!(filtered["class_count"], 1);
    assert_eq!(filtered["classes"][0]["clone_type"], "type-1");
    assert_eq!(filtered["summary"]["filtered_classes"], 1);
    assert_eq!(filtered["summary"]["total_classes"], 2);
}

#[test]
fn clone_classes_gate_exits_non_zero_only_when_a_class_is_reported() {
    let dir = fresh_temp_dir("clone-classes-gate");
    write(&dir, "clean.lisp", "(defun alpha (x) (+ x 1))\n");
    let root = dir.to_str().expect("utf-8 path");

    paredit()
        .args(["inspect", "clone-classes", "--fail-on-clones", root])
        .assert()
        .success();

    write(&dir, "dirty.lisp", "(defun beta (y) (+ y 2))\n");
    let failed = paredit()
        .args([
            "inspect",
            "clone-classes",
            "--threshold",
            "0.8",
            "--form-scope",
            "top-level",
            "--fail-on-clones",
            root,
        ])
        .output()
        .expect("run paredit");
    assert!(!failed.status.success());
    assert!(
        String::from_utf8_lossy(&failed.stderr).contains("clone-classes policy failed"),
        "{}",
        String::from_utf8_lossy(&failed.stderr)
    );
}

#[test]
fn clone_classes_rejects_a_min_members_below_two() {
    let dir = fresh_temp_dir("clone-classes-min-members");
    write(&dir, "a.lisp", "(defun alpha (x) (+ x 1))\n");

    let output = paredit()
        .args([
            "inspect",
            "clone-classes",
            "--min-members",
            "1",
            dir.to_str().expect("utf-8 path"),
        ])
        .output()
        .expect("run paredit");

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("--min-members must be at least 2"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ----------------------------------------------------------- clone-sequences

#[test]
fn clone_sequences_finds_a_run_no_whole_form_report_can_see() {
    let dir = fresh_temp_dir("clone-sequences-json");
    write(
        &dir,
        "svc.lisp",
        "\
(defun create (request)
  (validate request)
  (normalize request)
  (audit request)
  (persist request))
(defun update (request id)
  (assert-exists id)
  (validate request)
  (normalize request)
  (audit request)
  (merge-into id request))
",
    );
    let root = dir.to_str().expect("utf-8 path");

    let sequences = json_from(&["inspect", "clone-sequences", root]);
    let groups = sequences["groups"].as_array().expect("groups");
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0]["run_length"], 3);
    assert_eq!(groups[0]["occurrence_count"], 2);
    assert_eq!(groups[0]["clone_type"], "type-1");

    // The two enclosing definitions are not themselves a reported clone class,
    // which is the point: only the run repeats.
    let classes = json_from(&[
        "inspect",
        "clone-classes",
        "--form-scope",
        "top-level",
        root,
    ]);
    assert_eq!(classes["class_count"], 0);
}

#[test]
fn clone_sequences_ignores_same_shaped_calls_with_different_heads() {
    let dir = fresh_temp_dir("clone-sequences-heads");
    write(
        &dir,
        "a.lisp",
        "\
(defun alpha (x)
  (first-thing x)
  (second-thing x)
  (third-thing x)
  (done x))
(defun beta (y)
  (fourth-thing y)
  (fifth-thing y)
  (sixth-thing y)
  (finished y))
",
    );

    let report = json_from(&[
        "inspect",
        "clone-sequences",
        dir.to_str().expect("utf-8 path"),
    ]);

    assert_eq!(report["group_count"], 0);
}

#[test]
fn clone_sequences_rejects_a_run_length_below_two() {
    let dir = fresh_temp_dir("clone-sequences-bounds");
    write(&dir, "a.lisp", "(defun alpha (x) (+ x 1))\n");

    let output = paredit()
        .args([
            "inspect",
            "clone-sequences",
            "--min-run-length",
            "1",
            dir.to_str().expect("utf-8 path"),
        ])
        .output()
        .expect("run paredit");

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("--min-run-length must be at least 2"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ------------------------------------------------------------ clone-external

#[test]
fn clone_external_matches_across_head_symbols_and_corpora() {
    let dir = fresh_temp_dir("clone-external-json");
    write(
        &dir,
        "src/util.lisp",
        "(defun my-join (a b) (fold a b (list a b)))\n",
    );
    write(
        &dir,
        "refs/lib.lisp",
        "(defun str-join (x y) (fold x y (list x y)))\n",
    );

    let report = json_from(&[
        "inspect",
        "clone-external",
        "--threshold",
        "0.8",
        "--form-scope",
        "top-level",
        "--reference",
        dir.join("refs").to_str().expect("utf-8 path"),
        dir.join("src").to_str().expect("utf-8 path"),
    ]);

    let matches = report["matches"].as_array().expect("matches");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0]["clone_type"], "type-2");
    assert!(
        matches[0]["project"]["path"]
            .as_str()
            .expect("project path")
            .contains("util.lisp")
    );
    assert!(
        matches[0]["reference"]["path"]
            .as_str()
            .expect("reference path")
            .contains("lib.lisp")
    );
}

#[test]
fn clone_external_scans_a_vendor_reference_that_normal_discovery_would_skip() {
    // Every other command skips `vendor/` as a generated directory. A reference
    // corpus is almost always exactly such a directory, so this one must not.
    let dir = fresh_temp_dir("clone-external-vendor");
    write(&dir, "src/util.lisp", "(defun mine (a b) (fold a b))\n");
    write(&dir, "vendor/lib.lisp", "(defun theirs (x y) (fold x y))\n");

    let report = json_from(&[
        "inspect",
        "clone-external",
        "--threshold",
        "0.8",
        "--form-scope",
        "top-level",
        "--reference",
        dir.join("vendor").to_str().expect("utf-8 path"),
        dir.join("src").to_str().expect("utf-8 path"),
    ]);

    assert_eq!(report["summary"]["reference"]["candidates"], 1);
    assert_eq!(report["match_count"], 1);

    let skipped = json_from(&[
        "inspect",
        "clone-external",
        "--threshold",
        "0.8",
        "--form-scope",
        "top-level",
        "--reference-skip-generated",
        "--reference",
        dir.join("vendor").to_str().expect("utf-8 path"),
        dir.join("src").to_str().expect("utf-8 path"),
    ]);
    assert_eq!(skipped["summary"]["reference"]["candidates"], 0);
    assert_eq!(skipped["match_count"], 0);
}

#[test]
fn clone_external_requires_a_reference_corpus() {
    let dir = fresh_temp_dir("clone-external-required");
    write(&dir, "a.lisp", "(defun alpha (x) (+ x 1))\n");

    paredit()
        .args([
            "inspect",
            "clone-external",
            dir.to_str().expect("utf-8 path"),
        ])
        .assert()
        .failure();
}

// ----------------------------------------------------------- clone-threshold

#[test]
fn clone_threshold_reports_a_histogram_and_a_recommendation() {
    let dir = fresh_temp_dir("clone-threshold-json");
    let mut source = String::new();
    for index in 0..10 {
        source.push_str(&format!(
            "(defun name{index} (x) (+ x {index} {index} {index}))\n"
        ));
    }
    write(&dir, "a.lisp", &source);

    let report = json_from(&[
        "inspect",
        "clone-threshold",
        "--form-scope",
        "top-level",
        dir.to_str().expect("utf-8 path"),
    ]);

    assert_eq!(report["schema_version"], 1);
    let threshold = report["recommended_threshold"]
        .as_f64()
        .expect("recommended threshold");
    assert!((0.0..=1.0).contains(&threshold), "{threshold}");
    assert!(
        !report["histogram"]["buckets"]
            .as_array()
            .expect("buckets")
            .is_empty()
    );
    assert!(report["summary"]["scored_pairs"].as_u64().expect("scored") > 0);

    // Every candidate is a usable threshold with a pair count attached.
    for candidate in report["candidates"].as_array().expect("candidates") {
        let value = candidate["threshold"].as_f64().expect("threshold");
        assert!((0.0..=1.0).contains(&value), "{candidate}");
        assert!(candidate["method"].as_str().is_some());
    }
}

#[test]
fn clone_threshold_rejects_a_floor_outside_the_unit_interval() {
    let dir = fresh_temp_dir("clone-threshold-floor");
    write(&dir, "a.lisp", "(defun alpha (x) (+ x 1))\n");

    let output = paredit()
        .args([
            "inspect",
            "clone-threshold",
            "--floor",
            "1.5",
            dir.to_str().expect("utf-8 path"),
        ])
        .output()
        .expect("run paredit");

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("--floor must be between 0.0 and 1.0"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ---------------------------------------------------------- clone-genealogy

/// Whether a real repository can be built here.
///
/// The nix build sandbox has no `git` on `PATH`, and the genealogy ordering is
/// covered by domain tests against a fixture port regardless. What this
/// integration test adds is the `git blame` adapter, so it is worth running
/// where git exists and worth skipping where it does not, rather than pinning
/// the whole suite to an optional tool.
fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn git(dir: &Path, args: &[&str], date: Option<&str>) {
    let mut command = Command::new("git");
    command.current_dir(dir).args(args);
    if let Some(date) = date {
        command
            .env("GIT_AUTHOR_DATE", date)
            .env("GIT_COMMITTER_DATE", date);
    }
    let output = command.output().expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn clone_genealogy_names_the_older_copy_as_the_origin() {
    if !git_available() {
        return;
    }
    let dir = fresh_temp_dir("clone-genealogy");
    fs::create_dir_all(&dir).expect("create fixture directory");
    git(&dir, &["init", "-q", "."], None);
    git(
        &dir,
        &["config", "user.email", "test@example.invalid"],
        None,
    );
    git(&dir, &["config", "user.name", "Test"], None);
    git(&dir, &["config", "commit.gpgsign", "false"], None);

    write(&dir, "old.lisp", "(defun alpha (x) (+ x 1))\n");
    git(&dir, &["add", "-A"], None);
    git(
        &dir,
        &["commit", "-q", "--no-verify", "-m", "first"],
        Some("2023-01-15T00:00:00Z"),
    );

    write(&dir, "new.lisp", "(defun beta (y) (+ y 2))\n");
    git(&dir, &["add", "-A"], None);
    git(
        &dir,
        &["commit", "-q", "--no-verify", "-m", "second"],
        Some("2024-06-20T00:00:00Z"),
    );

    let report = json_from(&[
        "inspect",
        "clone-genealogy",
        "--threshold",
        "0.8",
        "--form-scope",
        "top-level",
        dir.to_str().expect("utf-8 path"),
    ]);

    let genealogies = report["genealogies"].as_array().expect("genealogies");
    assert_eq!(genealogies.len(), 1);
    let members = genealogies[0]["members"].as_array().expect("members");
    assert_eq!(members.len(), 2);
    assert_eq!(members[0]["role"], "origin");
    assert!(
        members[0]["form"]["path"]
            .as_str()
            .expect("origin path")
            .contains("old.lisp")
    );
    assert_eq!(members[0]["lag_days"], Value::Null);
    assert_eq!(members[1]["role"], "copy");
    assert_eq!(members[1]["lag_days"], 522);
    assert_eq!(genealogies[0]["span_days"], 522);
    assert_eq!(report["summary"]["undated_members"], 0);
}

#[test]
fn clone_genealogy_reports_unavailable_history_rather_than_inventing_it() {
    // Not a repository, so `git blame` has nothing to say. The report must say
    // so instead of dating every member to now.
    let dir = fresh_temp_dir("clone-genealogy-untracked");
    write(
        &dir,
        "a.lisp",
        "(defun alpha (x) (+ x 1))\n(defun beta (y) (+ y 2))\n",
    );

    let report = json_from(&[
        "inspect",
        "clone-genealogy",
        "--threshold",
        "0.8",
        "--form-scope",
        "top-level",
        dir.to_str().expect("utf-8 path"),
    ]);

    let genealogies = report["genealogies"].as_array().expect("genealogies");
    assert_eq!(genealogies.len(), 1);
    assert_eq!(genealogies[0]["dated_members"], 0);
    assert_eq!(genealogies[0]["span_days"], Value::Null);
    for member in genealogies[0]["members"].as_array().expect("members") {
        assert_eq!(member["role"], "unknown");
        assert!(member["unavailable"].as_str().is_some());
    }
    assert!(
        !report["summary"]["unavailable_reasons"]
            .as_array()
            .expect("reasons")
            .is_empty()
    );
}

// -------------------------------------------------- clone types on similarity

#[test]
fn similarity_pairs_carry_their_clone_type() {
    let dir = fresh_temp_dir("similarity-clone-type");
    write(
        &dir,
        "a.lisp",
        "(defun alpha (x) (+ x 1))\n(defun alpha (x) (+ x 1))\n",
    );

    let report = json_from(&[
        "inspect",
        "similarity",
        "--threshold",
        "0.8",
        "--form-scope",
        "top-level",
        dir.to_str().expect("utf-8 path"),
    ]);

    let pairs = report["pairs"].as_array().expect("pairs");
    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0]["clone_type"], "type-1");
    assert_eq!(pairs[0]["renamed_atoms"], 0);
    assert_eq!(pairs[0]["consistent_renaming"], true);
}

// ------------------------------------------------------------ shared surface

#[test]
fn every_clone_command_answers_in_both_output_formats() {
    let dir = fresh_temp_dir("clone-output-formats");
    write(
        &dir,
        "a.lisp",
        "(defun alpha (x) (+ x 1))\n(defun beta (y) (+ y 2))\n",
    );
    write(&dir, "refs/lib.lisp", "(defun gamma (z) (+ z 3))\n");
    let root = dir.to_str().expect("utf-8 path");
    let reference = dir.join("refs");

    for command in [
        "clone-classes",
        "clone-sequences",
        "clone-external",
        "clone-threshold",
        "clone-genealogy",
    ] {
        for format in ["json", "text"] {
            let mut args = vec!["inspect", command, "--output", format];
            if command == "clone-external" {
                args.push("--reference");
                args.push(reference.to_str().expect("utf-8 path"));
            }
            args.push(root);

            let output = paredit().args(&args).output().expect("run paredit");
            assert!(
                output.status.success(),
                "{command} --output {format} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let stdout = String::from_utf8_lossy(&output.stdout);
            if format == "json" {
                let value: Value = serde_json::from_str(&stdout).expect("valid JSON");
                assert_eq!(value["schema_version"], 1, "{command}");
            } else {
                assert!(
                    stdout.starts_with("schema_version\t1"),
                    "{command}: {stdout}"
                );
            }
        }
    }
}
