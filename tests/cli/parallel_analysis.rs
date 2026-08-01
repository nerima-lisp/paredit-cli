//! Multi-file analysis runs on every core, and says the same thing regardless.
//!
//! Section I item I12. Parallelism in a tool whose output is consumed by
//! machines is only worth having if it changes nothing but the clock, so the
//! tests here are almost entirely about *sameness*: the same bytes, the same
//! error, the same exit code, at every worker count.

use super::*;

use std::path::{Path, PathBuf};

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

/// Every failing file is reported, not only whichever worker lost the race
/// to finish first — otherwise the same tree produces a different set of
/// reported failures on different runs, and a caller cannot act on either.
/// (Section Q10: a broken file among many no longer takes the whole run
/// down with it, so both broken files are named on stderr and the 38 good
/// files' findings are still in the JSON report — at every worker count.)
#[test]
fn every_failing_file_is_reported_regardless_of_worker_count() {
    let dir = workspace("parallel-error-order", 40);
    // Two unparseable files among 40. Both must always be named.
    fs::write(dir.join("file005.lisp"), "(defun broken (x)\n").expect("write source");
    fs::write(dir.join("file030.lisp"), "(defun also-broken (x)\n").expect("write source");

    for jobs in ["1", "2", "8", "0"] {
        let mut command = paredit();
        let assertion = command
            .args(["inspect", "lint"])
            .arg(&dir)
            .args(["--jobs", jobs, "--output", "json"])
            .assert()
            .success();
        let stderr = String::from_utf8_lossy(&assertion.get_output().stderr).into_owned();
        let report: serde_json::Value =
            serde_json::from_slice(&assertion.get_output().stdout).expect("report is JSON");

        assert!(
            stderr.contains("file005.lisp") && stderr.contains("file030.lisp"),
            "--jobs {jobs} reported {stderr} rather than both failing files"
        );
        let failed_paths: Vec<&str> = report["partial_failures"]
            .as_array()
            .expect("partial_failures")
            .iter()
            .map(|failure| failure["file"].as_str().expect("file"))
            .collect();
        assert_eq!(failed_paths.len(), 2, "--jobs {jobs}: {failed_paths:?}");
        // The 38 other files still parsed, and each still trips both
        // redundant-progn and one-armed-if.
        assert_eq!(
            report["finding_count"].as_u64(),
            Some(76),
            "--jobs {jobs}: {report}"
        );
    }
}

/// A workspace whose first file carries far more work than all the others put
/// together, so the worker that owns it is the one that finishes last.
///
/// An even workload hides the bug this shape exists to catch: when every
/// worker takes the same time, a collector that appends each result as its
/// thread returns produces input order anyway, and the test passes for the
/// wrong reason. Loading the *first* file makes the *first* worker the
/// slowest — under every partitioning any of these commands use, contiguous
/// chunks or `skip(worker).step_by(workers)`, file000 belongs to worker 0 —
/// so an append-on-arrival collector would move its findings to the end of
/// the report, and an index-addressed one cannot.
fn heavy_first_workspace(label: &str, count: usize, heavy_definitions: usize) -> PathBuf {
    let dir = fresh_temp_dir(label);
    let mut heavy = String::new();
    for index in 0..heavy_definitions {
        // Varying the trailing list length keeps the forms structurally
        // distinct enough that similarity's overlap suppression stays cheap,
        // while the sheer count keeps the pair space above its split
        // threshold.
        let tail = (0..=index % 5)
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        heavy.push_str(&format!(
            "(defun heavy-{index} (x)\n  (let ((y{index} (helper-{index} x)))\n    (list y{index} {tail})))\n"
        ));
    }
    fs::write(dir.join("file000.lisp"), heavy).expect("write heavy source");
    for index in 1..count {
        fs::write(
            dir.join(format!("file{index:03}.lisp")),
            format!("(defun f{index} (x)\n  (let ((y (g{index} x)) (z 1))\n    (h{index} y z)))\n"),
        )
        .expect("write source");
    }
    dir
}

/// The workspace's files in the order a caller would name them.
fn sources_in_order(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .expect("read workspace")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "lisp"))
        .collect();
    files.sort();
    files
}

/// Runs `command` over `inputs` at one worker count and returns its stdout.
///
/// A `String` rather than the bytes so that a mismatch prints a diff someone
/// can read; these reports run to tens of kilobytes and a failed `Vec<u8>`
/// comparison renders every one of them as a decimal byte.
fn report_at(jobs: &str, command: &[&str], inputs: &[PathBuf]) -> String {
    let stdout = paredit()
        .args(command)
        .args(inputs)
        .args(["--jobs", jobs, "--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    String::from_utf8(stdout).expect("report is UTF-8")
}

/// The four commands that fan out on their own rather than through
/// `analyze_files`, checked for the property `analyze_files` is arranged
/// around: the report is the same bytes at every worker count.
///
/// Each of these had its own `thread::scope`, and each is checked over a
/// workspace whose first file finishes last — see [`heavy_first_workspace`].
#[test]
fn every_self_scheduled_command_is_byte_identical_at_every_worker_count() {
    for (label, command, whole_directory) in [
        ("parallel-lets", ["inspect", "lets"], false),
        (
            "parallel-unused-definitions",
            ["inspect", "unused-definitions"],
            false,
        ),
        (
            "parallel-remove-unused-definitions",
            ["refactor", "remove-unused-definitions"],
            false,
        ),
        ("parallel-similarity", ["inspect", "similarity"], true),
    ] {
        let dir = heavy_first_workspace(label, 11, 30);
        let inputs = if whole_directory {
            vec![dir.clone()]
        } else {
            sources_in_order(&dir)
        };
        let serial = report_at("1", &command, &inputs);

        for jobs in ["2", "3", "8", "0"] {
            assert_eq!(
                report_at(jobs, &command, &inputs),
                serial,
                "{} --jobs {jobs} produced different bytes from --jobs 1",
                command.join(" ")
            );
        }
    }
}

/// The similarity corpus has to be big enough that the worker threads are
/// actually spawned, or the byte-identity above is a statement about a serial
/// run. `SPLIT_MIN_PAIRS` in `packages/feature/similarity` is the threshold:
/// at or below it the pair comparison stays on one worker whatever `--jobs`
/// says.
#[test]
fn the_similarity_corpus_is_large_enough_to_spawn_workers() {
    const SPLIT_MIN_PAIRS: u64 = 2048;

    let dir = heavy_first_workspace("parallel-similarity-threshold", 11, 30);
    let report: serde_json::Value =
        serde_json::from_str(&report_at("4", &["inspect", "similarity"], &[dir]))
            .expect("report is JSON");
    let possible_pairs = report["summary"]["possible_pairs"]
        .as_u64()
        .expect("possible_pairs");

    assert!(
        possible_pairs > SPLIT_MIN_PAIRS,
        "the corpus yields {possible_pairs} possible pairs, at or below the {SPLIT_MIN_PAIRS} \
         split threshold, so the pair comparison never left one worker"
    );
    // No truncation either: a report cut off at `--max-results` would be
    // comparing whichever pairs survived, not the whole set.
    assert_eq!(report["summary"]["truncated"], serde_json::json!(false));
}

/// `--jobs` only reaches a fan-out that asks for it. Four call sites spawned
/// workers straight off `thread::available_parallelism()` instead, so
/// `--jobs 2` on a 64-core box ran 64 workers and no flag could say otherwise
/// — and nothing failed, because the *output* was right either way.
///
/// A source-level contract rather than a behavioural one for that exact
/// reason: worker count is deliberately unobservable in the report, so the
/// only thing that can catch the next site that forgets is the shape of the
/// call. Asking the machine is still allowed — it is what `--jobs 0` means —
/// but only in a file that read the flag first.
#[test]
fn every_worker_count_is_derived_from_the_jobs_flag() {
    const MACHINE: &str = "thread::available_parallelism(";
    const FLAG: &str = "effective_jobs()";

    let mut checked = 0usize;
    let mut stack = vec![PathBuf::from("packages"), PathBuf::from("src")];
    while let Some(path) = stack.pop() {
        if path.is_dir() {
            stack.extend(
                fs::read_dir(&path)
                    .expect("read source directory")
                    .flatten()
                    .map(|entry| entry.path()),
            );
            continue;
        }
        if !path.extension().is_some_and(|ext| ext == "rs") {
            continue;
        }
        let source = fs::read_to_string(&path).expect("read source file");
        if !source.contains(MACHINE) {
            continue;
        }
        checked += 1;
        assert!(
            source.contains(FLAG),
            "{} calls {MACHINE}..) without reading {FLAG}, so --jobs cannot bound it",
            path.display()
        );
    }

    assert!(
        checked >= 6,
        "only {checked} worker-count sites found; the walk is wrong"
    );
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
