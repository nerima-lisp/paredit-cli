//! `inspect external-diagnostics`: asking SBCL what it thinks of the code.
//!
//! Section I item I5. The parser and the baseline comparison are unit-tested
//! in their own packages against recorded transcripts, so the logic is covered
//! without an implementation installed. What is left for this file is the
//! wiring — argument validation, the gate, the baseline round trip — and the
//! parts of it that need SBCL are skipped rather than failed when it is
//! absent, because the Nix sandbox has no SBCL and neither does a fresh
//! contributor's machine.

use super::*;

use serde_json::Value;
use std::path::PathBuf;

/// Whether an SBCL is available to invoke.
///
/// Printed rather than silent when it is not: a suite that quietly stops
/// testing the interesting half is worse than one that says so.
fn sbcl_available() -> bool {
    let available = std::process::Command::new("sbcl")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    if !available {
        println!("skipping: no sbcl on PATH");
    }
    available
}

const SOURCE: &str = "(defun bar (x)\n  (+ missing x))\n\n(defun baz (unused)\n  1)\n";

fn workspace(label: &str, source: &str) -> PathBuf {
    let dir = fresh_temp_dir(label);
    fs::write(dir.join("demo.lisp"), source).expect("write source");
    dir
}

// --- argument contract, which needs no implementation ---

/// Compiling a file runs its macros. A caller must say which implementation
/// they are invoking rather than falling into one because a flag defaulted.
#[test]
fn the_implementation_has_no_default() {
    let dir = workspace("external-no-default", SOURCE);

    paredit()
        .current_dir(&dir)
        .args(["inspect", "external-diagnostics", "demo.lisp"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--implementation"));
}

#[test]
fn the_help_states_that_compiling_is_executing() {
    paredit()
        .args(["inspect", "external-diagnostics", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Compiling is executing"));
}

/// `--fail-on-introduced` without a baseline has nothing to compare against.
#[test]
fn failing_on_introduced_requires_a_baseline() {
    let dir = workspace("external-needs-baseline", SOURCE);

    paredit()
        .current_dir(&dir)
        .args(["inspect", "external-diagnostics", "demo.lisp"])
        .args(["--implementation", "sbcl", "--fail-on-introduced"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--baseline"));
}

#[test]
fn a_baseline_from_an_unsupported_schema_is_refused() {
    let dir = workspace("external-bad-baseline", SOURCE);
    fs::write(
        dir.join("before.json"),
        r#"{"schema_version": 99, "diagnostics": []}"#,
    )
    .expect("write baseline");

    paredit()
        .current_dir(&dir)
        .args(["inspect", "external-diagnostics", "demo.lisp"])
        .args(["--implementation", "sbcl", "--baseline", "before.json"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("schema version 99"));
}

/// A Clojure file cannot be compiled by a Common Lisp implementation, and the
/// report says so rather than reporting a clean bill of health.
#[test]
fn a_non_common_lisp_file_is_reported_as_unmodelled() {
    let dir = fresh_temp_dir("external-dialect");
    fs::write(dir.join("core.clj"), "(defn f [x] (inc x))\n").expect("write source");

    paredit()
        .current_dir(&dir)
        .args(["inspect", "external-diagnostics", "core.clj"])
        .args(["--implementation", "sbcl", "--output", "text"])
        .assert()
        .success()
        .stdout(predicate::str::contains("unmodelled"));
}

// --- the parts that invoke SBCL ---

#[test]
fn diagnostics_are_reported_at_the_definition_they_belong_to() {
    if !sbcl_available() {
        return;
    }
    let dir = workspace("external-report", SOURCE);

    let output = paredit()
        .current_dir(&dir)
        .args(["inspect", "external-diagnostics", "demo.lisp"])
        .args(["--implementation", "sbcl"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: Value = serde_json::from_slice(&output).expect("report is JSON");

    assert_eq!(report["report"], "external-diagnostics");
    let findings = report["files"][0]["findings"]
        .as_array()
        .expect("findings")
        .clone();
    assert!(
        findings.len() >= 2,
        "expected the undefined variable and the unused argument: {findings:#?}"
    );

    let undefined = findings
        .iter()
        .find(|finding| {
            finding["message"]
                .as_str()
                .is_some_and(|message| message.contains("undefined variable"))
        })
        .expect("the undefined variable is reported");
    assert_eq!(undefined["severity"], "warning");
    assert_eq!(undefined["line"], 1, "placed at (defun bar ...)");

    let unused = findings
        .iter()
        .find(|finding| {
            finding["message"]
                .as_str()
                .is_some_and(|message| message.contains("never used"))
        })
        .expect("the unused argument is reported");
    assert_eq!(
        unused["severity"], "style-warning",
        "a style-warning must not be reported as a warning"
    );
    assert_eq!(unused["line"], 4, "placed at (defun baz ...)");
}

/// The whole point of the command: gate a refactor on the diagnostics it
/// *introduced*, not on the ones it inherited.
#[test]
fn a_baseline_separates_introduced_diagnostics_from_inherited_ones() {
    if !sbcl_available() {
        return;
    }
    let dir = workspace("external-baseline", SOURCE);

    paredit()
        .current_dir(&dir)
        .args(["inspect", "external-diagnostics", "demo.lisp"])
        .args(["--implementation", "sbcl", "--save-baseline", "before.json"])
        .assert()
        .success();
    assert!(dir.join("before.json").is_file());

    // A refactor that renames a call to something that no longer exists.
    fs::write(
        dir.join("demo.lisp"),
        "(defun bar (x)\n  (+ missing x))\n\n(defun baz (unused)\n  (renamed-away 1))\n",
    )
    .expect("rewrite source");

    let assertion = paredit()
        .current_dir(&dir)
        .args(["inspect", "external-diagnostics", "demo.lisp"])
        .args(["--implementation", "sbcl", "--baseline", "before.json"])
        .args(["--fail-on-introduced"])
        .assert()
        // Exit 3 is the policy-gate code, distinct from a hard failure.
        .code(3);
    let report: Value =
        serde_json::from_slice(&assertion.get_output().stdout).expect("report is JSON");

    let findings = report["files"][0]["findings"].as_array().expect("findings");
    let introduced = findings
        .iter()
        .filter(|finding| finding["introduced"] == true)
        .collect::<Vec<_>>();

    assert_eq!(
        introduced.len(),
        1,
        "only the new diagnostic is introduced: {findings:#?}"
    );
    assert!(
        introduced[0]["message"]
            .as_str()
            .is_some_and(|message| message.contains("RENAMED-AWAY")),
        "unexpected introduced diagnostic: {:#?}",
        introduced[0]
    );

    // The headline count stays "what was analysed", not "what tripped the gate".
    assert_eq!(report["finding_count"], findings.len());
}

/// A run against an unchanged file must not report its own baseline back as a
/// regression.
#[test]
fn an_unchanged_file_introduces_nothing() {
    if !sbcl_available() {
        return;
    }
    let dir = workspace("external-unchanged", SOURCE);

    paredit()
        .current_dir(&dir)
        .args(["inspect", "external-diagnostics", "demo.lisp"])
        .args(["--implementation", "sbcl", "--save-baseline", "before.json"])
        .assert()
        .success();

    paredit()
        .current_dir(&dir)
        .args(["inspect", "external-diagnostics", "demo.lisp"])
        .args(["--implementation", "sbcl", "--baseline", "before.json"])
        .args(["--fail-on-introduced"])
        .assert()
        .success();
}

/// The source tree must be unchanged: this is an `inspect` command, and the
/// fasl belongs in a temporary directory.
#[test]
fn compiling_leaves_no_artifact_beside_the_source() {
    if !sbcl_available() {
        return;
    }
    let dir = workspace("external-clean", SOURCE);

    paredit()
        .current_dir(&dir)
        .args(["inspect", "external-diagnostics", "demo.lisp"])
        .args(["--implementation", "sbcl"])
        .assert()
        .success();

    let mut entries = fs::read_dir(&dir)
        .expect("read directory")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect::<Vec<_>>();
    entries.sort();
    assert_eq!(entries, ["demo.lisp"], "no fasl may be left in the tree");
    assert_eq!(
        fs::read_to_string(dir.join("demo.lisp")).expect("read source"),
        SOURCE
    );
}

/// A binary that does not exist must fail with the name it tried, not with a
/// clean report.
#[test]
fn a_missing_implementation_binary_is_an_error() {
    let dir = workspace("external-missing-binary", SOURCE);

    paredit()
        .current_dir(&dir)
        .args(["inspect", "external-diagnostics", "demo.lisp"])
        .args(["--implementation", "sbcl"])
        .args([
            "--implementation-path",
            "definitely-not-a-lisp-implementation",
        ])
        .assert()
        .failure();
}
