//! FR-005 (`--comment-column` / `format.comment-column`) and FR-006
//! (`--max-blank-lines` / `format.blank-lines-max-consecutive`), exercised
//! through the real binary. Mirrors `config_and_multi_file.rs`'s pattern for
//! Phase 1's flags: a flag-direct test in this module plus a paired
//! config-key test proving the same behavior is reachable from
//! `paredit.toml`.

use super::*;
use std::path::Path;

/// A repository-shaped scratch directory: a `.git` marker so configuration
/// discovery has a root to stop at, mirroring `config_and_multi_file.rs`.
fn repo(name: &str) -> PathBuf {
    let root = fresh_temp_dir(name);
    fs::create_dir_all(root.join(".git")).expect("create .git marker");
    root
}

#[test]
fn cli_format_comment_column_flag_is_off_by_default() {
    let dir = fresh_temp_dir("format-comment-column-off-by-default");
    let file = dir.join(Path::new("source.lisp"));
    fs::write(&file, "(a) ; one\n(bb) ; two\n").expect("write fixture");

    paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::eq("(a) ; one\n\n(bb) ; two\n"));
}

#[test]
fn cli_format_comment_column_flag_aligns_trailing_comments() {
    let dir = fresh_temp_dir("format-comment-column-flag");
    let file = dir.join(Path::new("source.lisp"));
    fs::write(&file, "(a) ; one\n(bb) ; two\n").expect("write fixture");

    // A fixed column applies uniformly, so it needs no `--max-blank-lines`
    // to see any effect (auto, column 0, is the mode that does).
    paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .arg("--comment-column")
        .arg("10")
        .assert()
        .success()
        .stdout(predicate::eq("(a)       ; one\n\n(bb)      ; two\n"));
}

#[test]
fn cli_format_comment_column_flag_rejects_out_of_range_values() {
    let dir = fresh_temp_dir("format-comment-column-out-of-range");
    let file = dir.join(Path::new("source.lisp"));
    fs::write(&file, "(a) ; one\n(bb) ; two\n").expect("write fixture");

    // Bounded to the same 0..=512 range as the `format.comment-column`
    // config key. Before this bound existed on the CLI flag, a value past
    // `usize::MAX` reached the formatter's `" ".repeat(padding)` call and
    // panicked with a capacity overflow instead of being rejected here.
    paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .arg("--comment-column")
        .arg("18446744073709551615")
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("invalid value"))
        .stderr(predicate::str::contains("not in 0..=512"));

    // A merely large, in-range-for-usize value that is still out of the
    // schema's bound: unchecked, this amplified a 30-byte input into
    // megabytes of padding rather than panicking.
    paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .arg("--comment-column")
        .arg("100000000")
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("invalid value"))
        .stderr(predicate::str::contains("not in 0..=512"));
}

#[test]
fn cli_format_comment_column_config_key_reaches_the_command() {
    let root = repo("format-config-comment-column");
    let input = "(a) ; one\n(bb) ; two\n";
    fs::write(root.join("source.lisp"), input).expect("write fixture");

    // With no configuration yet, the single-space default stands.
    paredit()
        .current_dir(&root)
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg("source.lisp")
        .assert()
        .success()
        .stdout(predicate::eq("(a) ; one\n\n(bb) ; two\n"));

    // `paredit.toml` opts in, so the column moves without anyone passing
    // `--comment-column` on the command line.
    fs::write(root.join("paredit.toml"), "[format]\ncomment-column = 10\n").expect("write config");
    paredit()
        .current_dir(&root)
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg("source.lisp")
        .assert()
        .success()
        .stdout(predicate::eq("(a)       ; one\n\n(bb)      ; two\n"));
}

#[test]
fn cli_format_max_blank_lines_flag_is_off_by_default() {
    let dir = fresh_temp_dir("format-max-blank-lines-off-by-default");
    let file = dir.join(Path::new("source.lisp"));
    fs::write(&file, "(a)\n\n\n(b)\n").expect("write fixture");

    paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::eq("(a)\n\n(b)\n"));
}

#[test]
fn cli_format_max_blank_lines_flag_preserves_blank_lines_up_to_the_maximum() {
    let dir = fresh_temp_dir("format-max-blank-lines-flag");
    let file = dir.join(Path::new("source.lisp"));
    fs::write(&file, "(a)\n\n\n(b)\n").expect("write fixture"); // two blank lines

    paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .arg("--max-blank-lines")
        .arg("2")
        .assert()
        .success()
        .stdout(predicate::eq("(a)\n\n\n(b)\n"));
}

#[test]
fn cli_format_max_blank_lines_flag_zero_removes_every_blank_line() {
    let dir = fresh_temp_dir("format-max-blank-lines-zero");
    let file = dir.join(Path::new("source.lisp"));
    fs::write(&file, "(a)\n\n\n(b)\n").expect("write fixture");

    paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .arg("--max-blank-lines")
        .arg("0")
        .assert()
        .success()
        .stdout(predicate::eq("(a)\n(b)\n"));
}

#[test]
fn cli_format_max_blank_lines_config_key_reaches_the_command() {
    let root = repo("format-config-max-blank-lines");
    let input = "(a)\n\n\n(b)\n"; // two blank lines
    fs::write(root.join("source.lisp"), input).expect("write fixture");

    // With no configuration yet, every gap collapses to exactly one blank
    // line, same as `--max-blank-lines` never being passed.
    paredit()
        .current_dir(&root)
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg("source.lisp")
        .assert()
        .success()
        .stdout(predicate::eq("(a)\n\n(b)\n"));

    // `paredit.toml` opts in, so both blank lines survive without anyone
    // passing `--max-blank-lines` on the command line.
    fs::write(
        root.join("paredit.toml"),
        "[format]\nblank-lines-max-consecutive = 2\n",
    )
    .expect("write config");
    paredit()
        .current_dir(&root)
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg("source.lisp")
        .assert()
        .success()
        .stdout(predicate::eq("(a)\n\n\n(b)\n"));
}
