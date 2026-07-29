//! The bounds every command accepts, and what happens when one is hit.
//!
//! `--timeout-ms` and the `--max-*` flags are declared once on the root
//! command as `global`, so these tests deliberately exercise them through
//! *different* subcommands: a flag that only works on the command it was
//! tested against is not a global flag.

use super::*;

/// The four size bounds and the budget must reach every namespace, not just
/// the one they were declared next to.
#[test]
fn the_budget_flags_reach_every_namespace() {
    let capabilities = capability_map();
    let expected = [
        "timeout-ms",
        "max-input-bytes",
        "max-file-bytes",
        "max-total-bytes",
        "max-files",
    ];

    for command in ["inspect check", "edit slurp-forward", "refactor plan"] {
        let flags = capabilities
            .get(command)
            .unwrap_or_else(|| panic!("{command} is missing from the capability map"));
        for flag in expected {
            assert!(
                flags.contains(flag),
                "{command} does not accept --{flag}; global budget flags must reach every command"
            );
        }
    }
}

#[test]
fn a_run_without_budget_flags_behaves_exactly_as_before() {
    let dir = fresh_temp_dir("budget-default");
    let source = dir.join("core.lisp");
    fs::write(&source, "(defun f (x) x)\n").expect("write source");

    paredit()
        .args(["inspect", "check", "--file"])
        .arg(&source)
        .assert()
        .success()
        .stdout(predicate::str::contains("ok"));
}

#[test]
fn an_input_over_the_lowered_ceiling_is_refused() {
    let dir = fresh_temp_dir("budget-input-bytes");
    let source = dir.join("core.lisp");
    fs::write(&source, "(defun f (x) x)\n").expect("write source");

    paredit()
        .args(["inspect", "check", "--file"])
        .arg(&source)
        .args(["--max-input-bytes", "4"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("input exceeds 4 bytes"));
}

/// The ratchet. A flag that could raise a ceiling would let a CI variable or a
/// checked-in config re-enable the exhaustion the bound exists to prevent.
#[test]
fn a_flag_may_not_raise_a_ceiling() {
    let dir = fresh_temp_dir("budget-raise");
    let source = dir.join("core.lisp");
    fs::write(&source, "(defun f (x) x)\n").expect("write source");

    paredit()
        .args(["inspect", "check", "--file"])
        .arg(&source)
        .args(["--max-input-bytes", "1GiB"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "limits may be lowered, never raised",
        ));
}

#[test]
fn a_byte_size_that_is_not_a_size_is_a_usage_error() {
    let dir = fresh_temp_dir("budget-unparsable");
    let source = dir.join("core.lisp");
    fs::write(&source, "(defun f (x) x)\n").expect("write source");

    paredit()
        .args(["inspect", "check", "--file"])
        .arg(&source)
        .args(["--max-total-bytes", "12 furlongs"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("is not a byte size"));
}

#[test]
fn both_unit_families_are_accepted() {
    let dir = fresh_temp_dir("budget-units");
    let source = dir.join("core.lisp");
    fs::write(&source, "(defun f (x) x)\n").expect("write source");

    for size in ["1MB", "1MiB", "1048576"] {
        paredit()
            .args(["inspect", "check", "--file"])
            .arg(&source)
            .args(["--max-input-bytes", size])
            .assert()
            .success();
    }
}

/// Zero is a real budget: it asks whether the run would start at all. It must
/// stop the work rather than being read as "no budget".
#[test]
fn a_zero_budget_stops_before_the_first_file() {
    let dir = fresh_temp_dir("budget-timeout-zero");
    let source = dir.join("core.lisp");
    fs::write(&source, "(defun f (x) x)\n").expect("write source");

    paredit()
        .args(["inspect", "check", "--file"])
        .arg(&source)
        .args(["--timeout-ms", "0"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("exceeded the 0ms budget"))
        .stderr(predicate::str::contains("core.lisp"));
}

#[test]
fn a_generous_budget_does_not_interfere() {
    let dir = fresh_temp_dir("budget-timeout-generous");
    let source = dir.join("core.lisp");
    fs::write(&source, "(defun f (x) x)\n").expect("write source");

    paredit()
        .args(["inspect", "check", "--file"])
        .arg(&source)
        .args(["--timeout-ms", "600000"])
        .assert()
        .success();
}

/// A timeout must name the file it was working on. "Timed out" with no scope
/// leaves a caller with a 50,000-file tree and nowhere to look.
#[test]
fn a_timeout_names_the_file_and_the_progress_made() {
    let dir = fresh_temp_dir("budget-timeout-scope");
    for index in 0..8 {
        fs::write(
            dir.join(format!("file{index}.lisp")),
            "(defun f (x) x)\n(defun g (y) y)\n",
        )
        .expect("write source");
    }

    let output = paredit()
        .args(["inspect", "lint"])
        .arg(&dir)
        .args(["--timeout-ms", "0"])
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    let stderr = String::from_utf8_lossy(&output);

    assert!(
        stderr.contains(".lisp"),
        "timeout must name a file: {stderr}"
    );
    assert!(
        stderr.contains("completed"),
        "timeout must report progress: {stderr}"
    );
}

/// A container states its budget through the environment, because the person
/// typing the command is not always the person who owns the memory.
#[test]
fn an_environment_variable_lowers_a_ceiling() {
    let dir = fresh_temp_dir("budget-env");
    let source = dir.join("core.lisp");
    fs::write(&source, "(defun f (x) x)\n").expect("write source");

    paredit()
        .args(["inspect", "check", "--file"])
        .arg(&source)
        .env("PAREDIT_MAX_INPUT_BYTES", "4")
        .assert()
        .failure()
        .stderr(predicate::str::contains("input exceeds 4 bytes"));
}

/// The environment is a ratchet too, and a bad value must name the variable
/// rather than a flag the operator never typed.
#[test]
fn a_bad_environment_value_names_the_variable() {
    let dir = fresh_temp_dir("budget-env-bad");
    let source = dir.join("core.lisp");
    fs::write(&source, "(defun f (x) x)\n").expect("write source");

    paredit()
        .args(["inspect", "check", "--file"])
        .arg(&source)
        .env("PAREDIT_MAX_FILE_BYTES", "not-a-size")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("PAREDIT_MAX_FILE_BYTES"));
}

/// A flag is more specific than the environment, so it wins — but only within
/// the ratchet: it still cannot raise what the environment lowered.
#[test]
fn a_flag_overrides_the_environment_within_the_ratchet() {
    let dir = fresh_temp_dir("budget-env-flag");
    let source = dir.join("core.lisp");
    fs::write(&source, "(defun f (x) x)\n").expect("write source");

    paredit()
        .args(["inspect", "check", "--file"])
        .arg(&source)
        .env("PAREDIT_MAX_INPUT_BYTES", "1MiB")
        .args(["--max-input-bytes", "4"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("input exceeds 4 bytes"));
}

/// The traversal bound, rather than the read bound: a directory scan that
/// finds more files than allowed must stop with a limit message.
#[test]
fn a_scan_over_the_file_ceiling_is_refused() {
    let dir = fresh_temp_dir("budget-max-files");
    for index in 0..6 {
        fs::write(dir.join(format!("file{index}.lisp")), "(defun f (x) x)\n")
            .expect("write source");
    }

    paredit()
        .args(["inspect", "lint"])
        .arg(&dir)
        .args(["--max-files", "2"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("2"));
}
