//! L4: `--verbosity` reaches the shared report envelope end to end.
//!
//! `packages/core/cli/src/report/render.rs`'s `print_text` implements
//! Quiet/Normal/Detailed once, generically, for every report that goes
//! through `print_report`. That generic implementation has its own unit
//! tests (`detail_lines_*` in `report/render.rs`), but nothing before this
//! proved the other half of the change: that the mechanical rollout across
//! 167 commands' `args.rs` → `workflow.rs` → `render.rs` wrapper actually
//! carries a parsed `--verbosity` value all the way to that function. A
//! wiring mistake in any one of those three files per command — the wrong
//! parameter position, a field that never got read — would compile (nothing
//! type-checks the *value* flows correctly) and pass every existing test
//! (none of which ever passed `--verbosity` before this change) while
//! silently ignoring the flag.
//!
//! `inspect todo` is the command under test: its `Finding` has four
//! `json_fields` (`marker`, `note`, `author`, `definition`), which makes
//! Detailed mode's extra lines easy to assert on individually, and its
//! fixture is a single source line.

use super::*;

const MARKED: &str = "(defun alpha ()\n  ;; TODO(dev): finish this\n  1)\n";

fn run(file: &std::path::Path, verbosity: &str) -> std::process::Output {
    paredit()
        .arg("inspect")
        .arg("todo")
        .arg("--output")
        .arg("text")
        .arg("--verbosity")
        .arg(verbosity)
        .arg(file)
        .output()
        .expect("run paredit")
}

#[test]
fn quiet_omits_the_finding_row_but_keeps_the_summary() {
    let dir = fresh_temp_dir("report-verbosity-quiet");
    let file = dir.join("a.lisp");
    fs::write(&file, MARKED).expect("write fixture");

    let output = run(&file, "quiet");
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");

    assert!(
        stdout.contains("finding_count\t1"),
        "quiet must still show the summary: {stdout:?}"
    );
    assert!(
        !stdout.contains("TODO"),
        "quiet must not print the finding row: {stdout:?}"
    );
    assert!(
        !stdout.contains("finish this"),
        "quiet must not print the finding row: {stdout:?}"
    );
}

#[test]
fn normal_shows_the_row_but_not_the_json_field_detail() {
    let dir = fresh_temp_dir("report-verbosity-normal");
    let file = dir.join("a.lisp");
    fs::write(&file, MARKED).expect("write fixture");

    let output = run(&file, "normal");
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");

    assert!(
        stdout.contains("finding_count\t1"),
        "normal must show the summary: {stdout:?}"
    );
    assert!(
        stdout.contains("TODO"),
        "normal must show the finding row: {stdout:?}"
    );
    assert!(
        !stdout.contains("\t\tmarker\t"),
        "normal must not show Detailed's per-field lines: {stdout:?}"
    );
}

#[test]
fn detailed_adds_every_json_field_under_the_row() {
    let dir = fresh_temp_dir("report-verbosity-detailed");
    let file = dir.join("a.lisp");
    fs::write(&file, MARKED).expect("write fixture");

    let output = run(&file, "detailed");
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");

    assert!(
        stdout.contains("TODO"),
        "detailed must still show the finding row: {stdout:?}"
    );
    for expected in [
        "\t\tmarker\tTODO",
        "\t\tnote\tfinish this",
        "\t\tauthor\tdev",
    ] {
        assert!(
            stdout.contains(expected),
            "detailed must show {expected:?} under the row: {stdout:?}"
        );
    }
}

#[test]
fn omitting_the_flag_defaults_to_normal() {
    let dir = fresh_temp_dir("report-verbosity-default");
    let file = dir.join("a.lisp");
    fs::write(&file, MARKED).expect("write fixture");

    let with_normal = run(&file, "normal").stdout;
    let omitted = paredit()
        .arg("inspect")
        .arg("todo")
        .arg("--output")
        .arg("text")
        .arg(&file)
        .output()
        .expect("run paredit")
        .stdout;

    assert_eq!(
        with_normal, omitted,
        "omitting --verbosity must be byte-identical to --verbosity normal"
    );
}
