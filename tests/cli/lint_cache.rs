//! `inspect lint --cache-dir`: not re-analyzing a file whose bytes are the same.
//!
//! Section I item I11. Every test here is a variation on one question — *can a
//! cached answer differ from a computed one* — because that is the only way a
//! cache can hurt. Speed is measured elsewhere; correctness is measured here.

use super::*;

use serde_json::Value;
use std::path::{Path, PathBuf};

/// A file with one finding (`(if x (progn y))` trips `redundant-progn`) plus
/// one clean file, so a report is neither empty nor uniform.
fn workspace(label: &str) -> PathBuf {
    let dir = fresh_temp_dir(label);
    fs::write(
        dir.join("flagged.lisp"),
        "(defun f (x)\n  (if x (progn (g x))))\n",
    )
    .expect("write source");
    fs::write(dir.join("clean.lisp"), "(defun h (x)\n  (+ x 1))\n").expect("write source");
    dir
}

fn lint(dir: &Path, cache: Option<&Path>) -> Vec<u8> {
    let mut command = paredit();
    command.args(["inspect", "lint"]).arg(dir);
    if let Some(cache) = cache {
        command.arg("--cache-dir").arg(cache);
    }
    command
        .args(["--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone()
}

/// The headline property: a cache changes the clock and nothing else.
#[test]
fn a_cached_run_reports_exactly_what_an_uncached_one_does() {
    let dir = workspace("cache-same");
    let cache = fresh_temp_dir("cache-same-store");

    let uncached = lint(&dir, None);
    let cold = lint(&dir, Some(&cache));
    let warm = lint(&dir, Some(&cache));

    assert_eq!(
        cold, uncached,
        "the populating run must match an uncached one"
    );
    assert_eq!(warm, uncached, "the warm run must match an uncached one");
}

/// The bug a 1065-file corpus surfaced. The key is the file's *content*, so two
/// files with identical bytes share one entry — which is correct, and is what
/// makes a renamed or copied file hit. Storing the path in the cached value
/// then served whichever duplicate wrote first, and every other copy was
/// reported under the wrong name.
#[test]
fn duplicate_files_are_reported_under_their_own_paths() {
    let dir = fresh_temp_dir("cache-duplicates");
    let cache = fresh_temp_dir("cache-duplicates-store");
    let source = "(defun f (x)\n  (if x (progn (g x))))\n";
    for name in ["a.lisp", "b.lisp", "c.lisp"] {
        fs::write(dir.join(name), source).expect("write identical source");
    }

    let cold: Value = serde_json::from_slice(&lint(&dir, Some(&cache))).expect("report is JSON");
    let warm: Value = serde_json::from_slice(&lint(&dir, Some(&cache))).expect("report is JSON");

    let paths = |report: &Value| {
        let mut paths = report["findings"]
            .as_array()
            .expect("findings")
            .iter()
            .map(|finding| finding["path"].as_str().unwrap_or_default().to_owned())
            .collect::<Vec<_>>();
        paths.sort();
        paths.dedup();
        paths
    };

    assert_eq!(
        paths(&cold).len(),
        3,
        "each duplicate reports its own finding"
    );
    assert_eq!(
        paths(&warm),
        paths(&cold),
        "a warm run must not report one duplicate's findings under another's path"
    );
}

/// One changed byte is a different question, so the answer must be recomputed.
#[test]
fn editing_a_file_invalidates_its_entry() {
    let dir = workspace("cache-edit");
    let cache = fresh_temp_dir("cache-edit-store");
    lint(&dir, Some(&cache));

    let redundant_progn_count = |report: &Value| {
        report["findings"]
            .as_array()
            .expect("findings")
            .iter()
            .filter(|finding| finding["rule"] == "redundant-progn")
            .count()
    };
    let before: Value = serde_json::from_slice(&lint(&dir, Some(&cache))).expect("report is JSON");
    assert_eq!(redundant_progn_count(&before), 1);

    // Remove the `progn`, and with it the finding.
    fs::write(dir.join("flagged.lisp"), "(defun f (x)\n  (if x (g x)))\n").expect("rewrite source");

    let after: Value = serde_json::from_slice(&lint(&dir, Some(&cache))).expect("report is JSON");
    assert_eq!(
        redundant_progn_count(&after),
        0,
        "the edited file must be re-analysed rather than served from the cache: {after}"
    );
}

/// A different rule selection is a different question. Without the rule set in
/// the key, a `--rule` run would serve a full-suite answer.
#[test]
fn a_different_rule_selection_does_not_share_entries() {
    let dir = workspace("cache-rules");
    let cache = fresh_temp_dir("cache-rules-store");

    let everything: Value =
        serde_json::from_slice(&lint(&dir, Some(&cache))).expect("report is JSON");

    let output = paredit()
        .args(["inspect", "lint"])
        .arg(&dir)
        .arg("--cache-dir")
        .arg(&cache)
        .args(["--rule", "empty-let", "--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let narrowed: Value = serde_json::from_slice(&output).expect("report is JSON");

    assert!(
        everything["finding_count"].as_u64() > narrowed["finding_count"].as_u64(),
        "a narrowed rule selection must not serve the full suite's answer: \
         {everything} vs {narrowed}"
    );
}

/// A baseline filters the answer; it is not part of the question. Changing one
/// must not throw the analysis away, which is why the *pre-baseline* findings
/// are what gets cached.
#[test]
fn a_baseline_still_applies_to_a_cached_run() {
    let dir = workspace("cache-baseline");
    let cache = fresh_temp_dir("cache-baseline-store");
    lint(&dir, Some(&cache));

    paredit()
        .args(["inspect", "lint"])
        .arg(&dir)
        .arg("--write-baseline")
        .arg(dir.join("baseline.json"))
        .assert()
        .success();

    let output = paredit()
        .args(["inspect", "lint"])
        .arg(&dir)
        .arg("--cache-dir")
        .arg(&cache)
        .arg("--baseline")
        .arg(dir.join("baseline.json"))
        .args(["--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: Value = serde_json::from_slice(&output).expect("report is JSON");

    assert_eq!(
        report["finding_count"], 0,
        "the baseline must filter a cached run as it filters an uncached one: {report}"
    );
}

/// A caller who asked for a cache and silently did not get one sees a run that
/// looks identical and is slower — the hardest configuration mistake to spot.
#[test]
fn an_unusable_cache_directory_is_an_error() {
    let dir = workspace("cache-unusable");
    // A regular file cannot become a directory.
    let blocked = dir.join("not-a-directory");
    fs::write(&blocked, "").expect("write blocker");

    paredit()
        .args(["inspect", "lint"])
        .arg(&dir)
        .arg("--cache-dir")
        .arg(&blocked)
        .assert()
        .failure()
        .stderr(predicate::str::contains("opening lint cache"));
}

/// The cache is an execution detail. Reporting it in the JSON would make the
/// report depend on whether a cache was configured, which is precisely what a
/// cache must never change.
#[test]
fn the_cache_reports_itself_on_stderr_and_not_in_the_report() {
    let dir = workspace("cache-stderr");
    let cache = fresh_temp_dir("cache-stderr-store");

    let assertion = paredit()
        .args(["inspect", "lint"])
        .arg(&dir)
        .arg("--cache-dir")
        .arg(&cache)
        .args(["--output", "json"])
        .assert()
        .success();
    let output = assertion.get_output();

    assert!(
        String::from_utf8_lossy(&output.stderr).contains("cache:"),
        "the run must say what the cache did"
    );

    // The comparison is against a run with no cache at all: identical bytes is
    // a stronger and more useful statement than "the word cache is absent",
    // which a rule description could satisfy by accident.
    assert_eq!(
        output.stdout,
        lint(&dir, None),
        "a cached run's report must be byte-identical to an uncached one"
    );
}
