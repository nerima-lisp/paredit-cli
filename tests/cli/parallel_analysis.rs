//! Multi-file analysis runs on every core, and says the same thing regardless.
//!
//! Section I item I12. Parallelism in a tool whose output is consumed by
//! machines is only worth having if it changes nothing but the clock, so the
//! tests here are almost entirely about *sameness*: the same bytes, the same
//! error, the same exit code, at every worker count.

use super::*;

use std::path::PathBuf;

/// Enough files to cross the threshold below which no thread is spawned.
fn workspace(label: &str, count: usize) -> PathBuf {
    let dir = fresh_temp_dir(label);
    for index in 0..count {
        fs::write(
            dir.join(format!("file{index:03}.lisp")),
            // `(if x (progn y))` trips redundant-progn, so every file has a
            // finding and the comparison is over real output rather than over
            // an empty report.
            format!("(defun f{index} (x)\n  (if x (progn (g{index} x))))\n"),
        )
        .expect("write source");
    }
    dir
}

fn lint(dir: &std::path::Path, jobs: &str) -> Vec<u8> {
    paredit()
        .args(["inspect", "lint"])
        .arg(dir)
        .args(["--jobs", jobs, "--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone()
}

/// The property the whole design is arranged around. Results are collected
/// into pre-indexed slots rather than pushed as they arrive, so the report
/// cannot depend on which worker finished first.
#[test]
fn the_report_is_byte_identical_at_every_worker_count() {
    let dir = workspace("parallel-identical", 40);
    let serial = lint(&dir, "1");

    for jobs in ["2", "3", "8", "0"] {
        assert_eq!(
            lint(&dir, jobs),
            serial,
            "--jobs {jobs} produced different bytes from --jobs 1"
        );
    }
}

/// A single worker must remain the serial path, not a pool of one: a caller
/// debugging a panic needs the original stack.
#[test]
fn one_worker_produces_the_same_report_as_the_default() {
    let dir = workspace("parallel-one", 20);
    assert_eq!(lint(&dir, "1"), lint(&dir, "0"));
}

/// A list too short to be worth a thread must still be analysed.
#[test]
fn a_short_file_list_is_analysed_below_the_parallel_threshold() {
    let dir = workspace("parallel-short", 3);
    let report: serde_json::Value =
        serde_json::from_slice(&lint(&dir, "0")).expect("report is JSON");

    assert!(
        report["finding_count"]
            .as_u64()
            .is_some_and(|count| count > 0),
        "three files must still be linted: {report}"
    );
}

/// A failure must be reported by *input order*, not by whichever worker lost
/// the race — otherwise the same tree produces different errors on different
/// runs, and a caller cannot act on either.
#[test]
fn the_first_failing_file_by_input_order_is_the_one_reported() {
    let dir = workspace("parallel-error-order", 40);
    // Two unparseable files. The earlier one must always be named.
    fs::write(dir.join("file005.lisp"), "(defun broken (x)\n").expect("write source");
    fs::write(dir.join("file030.lisp"), "(defun also-broken (x)\n").expect("write source");

    for jobs in ["1", "2", "8", "0"] {
        let output = paredit()
            .args(["inspect", "lint"])
            .arg(&dir)
            .args(["--jobs", jobs, "--output", "json"])
            .assert()
            .failure()
            .get_output()
            .stderr
            .clone();
        let stderr = String::from_utf8_lossy(&output);

        assert!(
            stderr.contains("file005.lisp"),
            "--jobs {jobs} reported {stderr} rather than the first failing file"
        );
    }
}

/// `--jobs` is a global flag like the rest of the budget, reachable from every
/// command rather than only from lint.
#[test]
fn jobs_is_available_on_every_command() {
    let capabilities = capability_map();
    for command in ["inspect check", "inspect lint", "refactor plan"] {
        assert!(
            capabilities
                .get(command)
                .is_some_and(|flags| flags.contains("jobs")),
            "{command} does not accept --jobs"
        );
    }
}
