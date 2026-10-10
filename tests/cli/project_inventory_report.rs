//! The reports whose subject is the project rather than a form in it.

use super::*;

const FIXTURE: &str = "(defpackage :app (:use :cl) (:export #:render #:missing-one))\n\
     (in-package :app)\n\
     (defun render (pane &key width) (list pane width))\n\
     (defun helper () (render 1 :widht 2))\n\
     (deftest test-render () 1)\n\
     (defun dead () (return-from dead 1) (never-runs))\n";

const SYSTEM_FIXTURE: &str = "(defsystem \"app\"\n\
     \x20 :license \"MIT\"\n\
     \x20 :serial t\n\
     \x20 :depends-on (\"alexandria\" (:version \"bordeaux-threads\" \"0.9\"))\n\
     \x20 :components ((:file \"a\") (:file \"b\" :depends-on (\"a\"))))\n\
     (defsystem \"app/tests\" :license \"GPL-3.0\" :depends-on (\"app\"))\n";

fn fixture(name: &str) -> PathBuf {
    let dir = fresh_temp_dir(name);
    let file = dir.join("core.lisp");
    fs::write(&file, FIXTURE).expect("write lisp fixture");
    file
}

fn system_fixture(name: &str) -> PathBuf {
    let dir = fresh_temp_dir(name);
    let file = dir.join("app.asd");
    fs::write(&file, SYSTEM_FIXTURE).expect("write asd fixture");
    file
}

const COMMANDS: [&str; 9] = [
    "api-surface",
    "test-map",
    "symbol-index",
    "keyword-arity",
    "unreachable-expressions",
    "external-systems",
    "licenses",
    "serial-consistency",
    "blame",
];

#[test]
fn cli_api_surface_pairs_an_export_with_its_signature() {
    paredit()
        .args(["inspect", "api-surface", "--output", "json"])
        .arg(fixture("inspect-api-surface"))
        .assert()
        .success()
        .stdout(predicate::str::contains("\"name\": \"RENDER\""))
        .stdout(predicate::str::contains("\"signature\": \"defun/1..3\""))
        .stdout(predicate::str::contains("\"kind\": \"undefined-export\""));
}

/// The full release-check loop: snapshot, change, compare. Pinned end to end
/// because the two commands share a JSON contract that nothing else checks.
#[test]
fn cli_api_diff_reads_an_api_surface_snapshot_and_answers_the_semver_question() {
    let dir = fresh_temp_dir("inspect-api-diff");
    let file = dir.join("core.lisp");
    fs::write(&file, "(defpackage :app (:export #:f))\n(defun f (a) a)\n")
        .expect("write lisp fixture");

    let baseline = dir.join("baseline.json");
    let snapshot = paredit()
        .args(["inspect", "api-surface", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    fs::write(&baseline, snapshot).expect("write baseline");

    // Raising the minimum arity breaks every existing caller.
    fs::write(
        &file,
        "(defpackage :app (:export #:f))\n(defun f (a b) (list a b))\n",
    )
    .expect("rewrite lisp fixture");

    paredit()
        .args(["inspect", "api-diff", "--output", "json", "--baseline"])
        .arg(&baseline)
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"impact\": \"breaking\""))
        .stdout(predicate::str::contains("\"required_bump\": \"major\""));
}

#[test]
fn cli_api_diff_fails_when_the_change_needs_a_bigger_bump_than_intended() {
    let dir = fresh_temp_dir("inspect-api-diff-gate");
    let file = dir.join("core.lisp");
    let baseline = dir.join("baseline.json");
    fs::write(
        &baseline,
        r#"{"files":[{"findings":[{"name":"F","package":"APP","category":"defun","required_arity":1,"max_arity":1}]}]}"#,
    )
    .expect("write baseline");
    fs::write(
        &file,
        "(defpackage :app (:export #:f))\n(defun f (a b) (list a b))\n",
    )
    .expect("write lisp fixture");

    paredit()
        .args([
            "inspect",
            "api-diff",
            "--intended-bump",
            "minor",
            "--baseline",
        ])
        .arg(&baseline)
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("inspect api-diff policy failed"));
}

#[test]
fn cli_test_map_pairs_a_test_with_its_subject() {
    paredit()
        .args(["inspect", "test-map", "--output", "json"])
        .arg(fixture("inspect-test-map"))
        .assert()
        .success()
        .stdout(predicate::str::contains("\"coverage\": \"tested\""))
        .stdout(predicate::str::contains("\"TEST-RENDER\""))
        .stdout(predicate::str::contains("\"coverage\": \"untested\""));
}

#[test]
fn cli_test_map_joins_separate_files_without_changing_them() {
    let dir = fresh_temp_dir("inspect-test-map-cross-file");
    let source = dir.join("source.lisp");
    let tests = dir.join("tests.lisp");
    let source_text = "(in-package :app)\n(defun render (x) x)\n";
    let test_text = "(in-package :app)\n(deftest test-render () 1)\n";
    fs::write(&source, source_text).expect("write source");
    fs::write(&tests, test_text).expect("write tests");

    let first = paredit()
        .args([
            "inspect",
            "test-map",
            "--output",
            "json",
            "--fail-on-untested",
        ])
        .arg(&source)
        .arg(&tests)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let reversed = paredit()
        .args([
            "inspect",
            "test-map",
            "--output",
            "json",
            "--fail-on-untested",
        ])
        .arg(&tests)
        .arg(&source)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert_eq!(first, reversed);
    let report: serde_json::Value = serde_json::from_slice(&first).expect("JSON report");
    assert_eq!(report["file_count"], 2);
    assert_eq!(report["policy"]["passed"], true);
    assert_eq!(report["files"][0]["findings"][0]["coverage"], "tested");
    assert_eq!(
        report["files"][0]["findings"][0]["tests"],
        serde_json::json!(["TEST-RENDER"])
    );
    assert_eq!(report["files"][1]["findings"], serde_json::json!([]));
    assert_eq!(
        fs::read_to_string(source).expect("read source"),
        source_text
    );
    assert_eq!(fs::read_to_string(tests).expect("read tests"), test_text);
}

#[test]
fn cli_test_map_does_not_join_different_reader_packages() {
    let dir = fresh_temp_dir("inspect-test-map-package-boundary");
    let source = dir.join("source.lisp");
    let tests = dir.join("tests.lisp");
    fs::write(&source, "(in-package :app)\n(defun render (x) x)\n").expect("write source");
    fs::write(&tests, "(in-package :other)\n(deftest test-render () 1)\n").expect("write tests");
    let output = paredit()
        .args([
            "inspect",
            "test-map",
            "--output",
            "json",
            "--fail-on-untested",
        ])
        .arg(&source)
        .arg(&tests)
        .assert()
        .code(3)
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).expect("JSON report");
    assert_eq!(report["policy"]["passed"], false);
    assert_eq!(report["files"][0]["findings"][0]["coverage"], "untested");
    assert_eq!(
        report["files"][1]["findings"][0]["coverage"],
        "unattributed"
    );
}

#[test]
fn cli_keyword_arity_reports_a_misspelled_keyword_by_name() {
    paredit()
        .args(["inspect", "keyword-arity", "--output", "json"])
        .arg(fixture("inspect-keyword-arity"))
        .assert()
        .success()
        .stdout(predicate::str::contains("\"fault\": \"unknown-keyword\""))
        .stdout(predicate::str::contains("\"keyword\": \":widht\""));
}

#[test]
fn cli_unreachable_expressions_reports_a_form_after_a_return() {
    paredit()
        .args(["inspect", "unreachable-expressions", "--output", "json"])
        .arg(fixture("inspect-unreachable"))
        .assert()
        .success()
        .stdout(predicate::str::contains("\"after\": \"return-from\""))
        .stdout(predicate::str::contains("(never-runs)"));
}

#[test]
fn cli_symbol_index_separates_defined_symbols_from_external_ones() {
    paredit()
        .args(["inspect", "symbol-index", "--output", "json"])
        .arg(fixture("inspect-symbol-index"))
        .assert()
        .success()
        .stdout(predicate::str::contains("\"kind\": \"defined\""))
        .stdout(predicate::str::contains("\"kind\": \"external\""))
        .stdout(predicate::str::contains("\"occurrences\""));
}

#[test]
fn cli_external_systems_reads_both_depends_on_spellings() {
    paredit()
        .args(["inspect", "external-systems", "--output", "json"])
        .arg(system_fixture("inspect-external-systems"))
        .assert()
        .success()
        .stdout(predicate::str::contains("\"name\": \"alexandria\""))
        .stdout(predicate::str::contains("\"name\": \"bordeaux-threads\""))
        .stdout(predicate::str::contains("\"version\": \"0.9\""))
        .stdout(predicate::str::contains("\"kind\": \"internal\""));
}

#[test]
fn cli_licenses_reports_a_permissive_system_superseded_by_a_gpl_one() {
    paredit()
        .args(["inspect", "licenses", "--output", "json"])
        .arg(system_fixture("inspect-licenses"))
        .assert()
        .success()
        .stdout(predicate::str::contains("\"copyleft\": \"permissive\""))
        .stdout(predicate::str::contains(
            "\"copyleft\": \"strong-copyleft\"",
        ))
        .stdout(predicate::str::contains("\"superseded_by\": \"app/tests\""));
}

#[test]
fn cli_serial_consistency_reports_a_redundant_dependency() {
    paredit()
        .args(["inspect", "serial-consistency", "--output", "json"])
        .arg(system_fixture("inspect-serial-consistency"))
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"fault\": \"redundant-dependency\"",
        ));
}

/// The fixture lives in a temp directory, which is not a git repository, so
/// this pins the fallback: blame is unavailable and the report says so instead
/// of emitting an empty author.
#[test]
fn cli_blame_says_so_when_git_cannot_answer() {
    paredit()
        .args(["inspect", "blame", "--output", "json"])
        .arg(fixture("inspect-blame"))
        .assert()
        .success()
        .stdout(predicate::str::contains("blame_unavailable"))
        .stdout(predicate::str::contains("\"kind\": \"unattributed\""));
}

#[test]
fn cli_api_surface_fail_on_undefined_export_trips_gate() {
    paredit()
        .args(["inspect", "api-surface", "--fail-on-undefined-export"])
        .arg(fixture("inspect-api-surface-gate"))
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "inspect api-surface policy failed",
        ));
}

#[test]
fn cli_every_project_inventory_report_is_byte_identical_across_runs() {
    let lisp = fixture("inspect-inventory-determinism");
    let system = system_fixture("inspect-inventory-determinism-asd");

    for command in COMMANDS {
        let target = if matches!(
            command,
            "external-systems" | "licenses" | "serial-consistency"
        ) {
            &system
        } else {
            &lisp
        };
        let run = || {
            paredit()
                .args(["inspect", command, "--output", "json"])
                .arg(target)
                .assert()
                .success()
                .get_output()
                .stdout
                .clone()
        };
        assert_eq!(run(), run(), "{command} is not deterministic");
    }
}

#[test]
fn cli_every_project_inventory_report_names_itself_in_its_output() {
    let file = fixture("inspect-inventory-self-naming");

    for command in ["api-surface", "test-map", "symbol-index", "blame"] {
        paredit()
            .args(["inspect", command, "--output", "json"])
            .arg(&file)
            .assert()
            .success()
            .stdout(predicate::str::contains(format!(
                "\"report\": \"inspect {command}\""
            )));
    }
}

#[test]
fn cli_dotfiles_inventories_metadata_without_reading_rule_contents() {
    let root = fresh_temp_dir("inspect-dotfiles-metadata");
    let rules = root.join(".paredit/rules");
    fs::create_dir_all(&rules).expect("create rules directory");
    let rule = rules.join("broken.lisp");
    let bytes = b"\xff(not valid Lisp CONTENT_MUST_NOT_APPEAR";
    fs::write(&rule, bytes).expect("write invalid rule");
    let run = || {
        paredit()
            .args(["inspect", "dotfiles", "--output", "json", "--root"])
            .arg(&root)
            .assert()
            .success()
            .get_output()
            .stdout
            .clone()
    };
    let output = run();
    assert_eq!(output, run());
    let report: serde_json::Value = serde_json::from_slice(&output).expect("JSON report");
    assert_eq!(report["present"], true);
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["complete"], true);
    assert_eq!(report["diagnostics"], serde_json::json!([]));
    let entries = report["entries"].as_array().expect("entries");
    let entry = entries
        .iter()
        .find(|entry| entry["path"] == ".paredit/rules/broken.lisp")
        .expect("rule metadata");
    assert_eq!(entry["kind"], "file");
    assert_eq!(entry["category"], "rule");
    assert_eq!(entry["status"], "inventoried");
    assert_eq!(entry["bytes"], bytes.len());
    assert!(
        !String::from_utf8(output)
            .expect("UTF-8 report")
            .contains("CONTENT_MUST_NOT_APPEAR")
    );
    assert_eq!(fs::read(rule).expect("read unchanged rule"), bytes);
}

#[cfg(unix)]
#[test]
fn cli_dotfiles_does_not_follow_or_disclose_symlink_targets() {
    let root = fresh_temp_dir("inspect-dotfiles-symlink");
    let external = fresh_temp_dir("inspect-dotfiles-external");
    fs::create_dir(root.join(".paredit")).expect("create dotfiles directory");
    fs::write(external.join("private.lisp"), "PRIVATE_BODY").expect("write external file");
    std::os::unix::fs::symlink(&external, root.join(".paredit/rules")).expect("create symlink");
    let output = paredit()
        .args(["inspect", "dotfiles", "--output", "json", "--root"])
        .arg(&root)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).expect("JSON report");
    let entries = report["entries"].as_array().expect("entries");
    let link = entries
        .iter()
        .find(|entry| entry["path"] == ".paredit/rules")
        .expect("symlink entry");
    assert_eq!(link["kind"], "symlink");
    assert_eq!(link["status"], "not_followed");
    assert!(!entries.iter().any(|entry| {
        entry["path"]
            .as_str()
            .expect("entry path")
            .contains("private.lisp")
    }));
    let text = String::from_utf8(output).expect("UTF-8 report");
    assert!(!text.contains("PRIVATE_BODY"));
    assert!(!text.contains(external.to_str().expect("external path")));
}

#[test]
fn cli_dotfiles_reports_absence_without_creating_a_directory() {
    let root = fresh_temp_dir("inspect-dotfiles-absent");
    let output = paredit()
        .args(["inspect", "dotfiles", "--output", "json", "--root"])
        .arg(&root)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).expect("JSON report");
    assert_eq!(
        report,
        serde_json::json!({"schema_version": 1, "present": false, "complete": true, "entries": [], "diagnostics": []})
    );
    assert!(!root.join(".paredit").exists());
}

#[test]
fn cli_dotfiles_emits_incomplete_diagnostics_before_failing_the_gate() {
    let root = fresh_temp_dir("inspect-dotfiles-limit");
    fs::create_dir(root.join(".paredit")).expect("create dotfiles directory");
    let file = fs::File::create(root.join(".paredit/large.bin")).expect("create large file");
    file.set_len(16 * 1024 * 1024 + 1)
        .expect("set sparse file length");
    let output = paredit()
        .args(["inspect", "dotfiles", "--output", "json", "--root"])
        .arg(&root)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("inventory is incomplete"))
        .get_output()
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON report");
    let error: serde_json::Value = serde_json::from_slice(&output.stderr).expect("JSON error");
    assert_eq!(error["error"]["code"], "gate.failed");
    assert_eq!(error["error"]["category"], "gate");
    assert_eq!(error["error"]["exit_code"], 3);
    assert_eq!(report["complete"], false);
    assert_eq!(report["schema_version"], 1);
    assert!(
        report["diagnostics"]
            .as_array()
            .expect("diagnostics")
            .iter()
            .any(|diagnostic| diagnostic["code"] == "file_size_limit")
    );
    assert_eq!(
        fs::metadata(root.join(".paredit/large.bin"))
            .expect("unchanged metadata")
            .len(),
        16 * 1024 * 1024 + 1
    );
}

#[test]
fn cli_dotfiles_fails_for_a_missing_root_without_printing_a_report() {
    let root = fresh_temp_dir("inspect-dotfiles-missing-root").join("missing");
    paredit()
        .args(["inspect", "dotfiles", "--output", "json", "--root"])
        .arg(&root)
        .assert()
        .failure()
        .stdout("");
    assert!(!root.exists());
}
