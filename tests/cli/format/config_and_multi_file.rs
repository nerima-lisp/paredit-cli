//! FR-001 (`format.max-inline-width` config), FR-003 (workspace `--diff-stat`
//! aggregation), and FR-004 (`--reindent-block-comments`), exercised through
//! the real binary.

use super::*;
use std::path::Path;

/// A repository-shaped scratch directory: a `.git` marker so configuration
/// discovery has a root to stop at, mirroring `tests/cli/config_command.rs`.
fn repo(name: &str) -> PathBuf {
    let root = fresh_temp_dir(name);
    fs::create_dir_all(root.join(".git")).expect("create .git marker");
    root
}

#[test]
fn cli_format_max_inline_width_config_key_reaches_the_command() {
    // Before FR-001, `format.max-inline-width` was declared in the schema but
    // had no binding to `--max-width`, so setting it in `paredit.toml` did
    // nothing at all.
    let root = repo("format-config-max-inline-width");
    let input = "(some-function-name argument-one argument-two argument-three argument-four argument-five)\n";
    fs::write(root.join("source.lisp"), input).expect("write fixture");

    // With no configuration yet, this is past the compiled-in 80-column
    // default and must wrap.
    paredit()
        .current_dir(&root)
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg("source.lisp")
        .assert()
        .success()
        .stdout(predicate::eq(input).not());

    // `paredit.toml` widens the budget past 89 columns, so it must fit on one
    // line without anyone passing `--max-width` on the command line.
    fs::write(
        root.join("paredit.toml"),
        "[format]\nmax-inline-width = 200\n",
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
        .stdout(predicate::eq(input));
}

#[test]
fn cli_format_block_comment_reindent_config_key_reaches_the_command() {
    // Mirrors `cli_format_max_inline_width_config_key_reaches_the_command`:
    // `format.block-comment-reindent` must reach `edit format` the same way
    // `--reindent-block-comments` does when passed directly on the command
    // line.
    let root = repo("format-config-block-comment-reindent");
    let input = "   #| Header\n      continuation\n   |#\n(foo)\n";
    fs::write(root.join("source.lisp"), input).expect("write fixture");

    // With no configuration yet, the interior lines are left untouched.
    paredit()
        .current_dir(&root)
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg("source.lisp")
        .assert()
        .success()
        .stdout(predicate::eq(
            "#| Header\n      continuation\n   |#\n(foo)\n",
        ));

    // `paredit.toml` opts in, so the interior lines realign without anyone
    // passing `--reindent-block-comments` on the command line.
    fs::write(
        root.join("paredit.toml"),
        "[format]\nblock-comment-reindent = true\n",
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
        .stdout(predicate::eq("#| Header\n   continuation\n|#\n(foo)\n"));
}

#[test]
fn cli_format_diff_stat_aggregates_across_multiple_files() {
    let dir = fresh_temp_dir("format-diff-stat-multi");
    let changed_one = dir.join(Path::new("a.lisp"));
    let changed_two = dir.join(Path::new("b.lisp"));
    let unchanged = dir.join(Path::new("c.lisp"));
    fs::write(&changed_one, "(defun a   ()   1)\n").expect("write a.lisp");
    fs::write(&changed_two, "(defun b   ()   2)\n").expect("write b.lisp");
    fs::write(&unchanged, "(defun c ()\n  3)\n").expect("write c.lisp");

    let output = paredit()
        .arg("edit")
        .arg("format")
        .arg("--diff-stat")
        .arg(&changed_one)
        .arg(&changed_two)
        .arg(&unchanged)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value =
        serde_json::from_slice(&output).expect("--diff-stat emits valid JSON");

    assert_eq!(report["total_files"], 3);
    assert_eq!(report["files_changed"], 2);
    assert!(report["hunks"].as_u64().expect("hunks") >= 2);
    assert!(report["added_lines"].as_u64().expect("added_lines") > 0);
    assert!(report["removed_lines"].as_u64().expect("removed_lines") > 0);

    let files = report["files"].as_array().expect("files array");
    assert_eq!(files.len(), 3);
    let by_path = |name: &str| {
        files
            .iter()
            .find(|entry| entry["path"].as_str().is_some_and(|p| p.ends_with(name)))
            .unwrap_or_else(|| panic!("no entry for {name} in {files:?}"))
    };
    assert_eq!(by_path("a.lisp")["changed"], true);
    assert_eq!(by_path("b.lisp")["changed"], true);
    assert_eq!(by_path("c.lisp")["changed"], false);
    assert_eq!(by_path("c.lisp")["hunks"], 0);

    // Aggregating never writes any of the files.
    assert_eq!(
        fs::read_to_string(&unchanged).expect("read c.lisp"),
        "(defun c ()\n  3)\n"
    );
}

#[test]
fn cli_format_diff_stat_multi_file_reports_zero_changes_when_nothing_needs_reformatting() {
    // Every file is already canonically formatted, so the aggregate summary
    // must report zero across the board rather than erroring or omitting
    // the summary line.
    let dir = fresh_temp_dir("format-diff-stat-multi-zero-changes");
    let a = dir.join(Path::new("a.lisp"));
    let b = dir.join(Path::new("b.lisp"));
    let c = dir.join(Path::new("c.lisp"));
    fs::write(&a, "(defun a ()\n  1)\n").expect("write a.lisp");
    fs::write(&b, "(defun b ()\n  2)\n").expect("write b.lisp");
    fs::write(&c, "(defun c ()\n  3)\n").expect("write c.lisp");

    let output = paredit()
        .arg("edit")
        .arg("format")
        .arg("--diff-stat")
        .arg(&a)
        .arg(&b)
        .arg(&c)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value =
        serde_json::from_slice(&output).expect("--diff-stat emits valid JSON");

    assert_eq!(report["total_files"], 3);
    assert_eq!(report["files_changed"], 0);
    assert_eq!(report["hunks"], 0);
    assert_eq!(report["added_lines"], 0);
    assert_eq!(report["removed_lines"], 0);

    let files = report["files"].as_array().expect("files array");
    assert_eq!(files.len(), 3);
    for entry in files {
        assert_eq!(entry["changed"], false);
        assert_eq!(entry["hunks"], 0);
    }

    // Aggregating never writes any of the files.
    for (path, contents) in [
        (&a, "(defun a ()\n  1)\n"),
        (&b, "(defun b ()\n  2)\n"),
        (&c, "(defun c ()\n  3)\n"),
    ] {
        assert_eq!(fs::read_to_string(path).expect("read file"), contents);
    }
}

#[test]
fn cli_format_diff_stat_multi_file_positional_paths_require_diff_stat() {
    let dir = fresh_temp_dir("format-diff-stat-multi-requires-flag");
    let a = dir.join(Path::new("a.lisp"));
    let b = dir.join(Path::new("b.lisp"));
    fs::write(&a, "(a)\n").expect("write a.lisp");
    fs::write(&b, "(b)\n").expect("write b.lisp");

    paredit()
        .arg("edit")
        .arg("format")
        .arg(&a)
        .arg(&b)
        .assert()
        .failure();
}

#[test]
fn cli_format_reindent_block_comments_flag_realigns_a_leading_block_comment() {
    let dir = fresh_temp_dir("format-reindent-block-comments");
    let file = dir.join(Path::new("source.lisp"));
    fs::write(&file, "   #| Header\n      continuation\n   |#\n(foo)\n").expect("write fixture");

    // Off by default: the comment's interior lines are untouched.
    paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::eq(
            "#| Header\n      continuation\n   |#\n(foo)\n",
        ));

    // Opted in: interior lines realign to the comment's new depth, keeping
    // their indentation relative to each other.
    paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .arg("--reindent-block-comments")
        .assert()
        .success()
        .stdout(predicate::eq("#| Header\n   continuation\n|#\n(foo)\n"));
}
