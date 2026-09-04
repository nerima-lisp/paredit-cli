use super::*;
use std::path::Path;

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

    paredit()
        .current_dir(&root)
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg("source.lisp")
        .assert()
        .success()
        .stdout(predicate::eq("(a) ; one\n\n(bb) ; two\n"));

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
fn cli_format_emacs_lisp_preserves_adjacent_top_level_forms() {
    let dir = fresh_temp_dir("format-elisp-adjacent-top-level-forms");
    let file = dir.join(Path::new("source.el"));
    let input = ";;; source.el --- Example -*- lexical-binding: t; -*-\n\n;; Copyright Example\n\n;; Commentary:\n\n;; Example package.\n\n;;; Code:\n\n(require 'cl-lib)\n(require 'seq)\n\n(declare-function example \"example\")\n(declare-function other \"other\")\n";
    fs::write(&file, input).expect("write fixture");

    paredit()
        .arg("edit")
        .arg("format")
        .arg("--dialect")
        .arg("emacs-lisp")
        .arg("--file")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::eq(input));
}

#[test]
fn cli_format_emacs_lisp_preserves_blank_lines_in_progn_body() {
    let dir = fresh_temp_dir("format-elisp-progn-blank-lines");
    let file = dir.join(Path::new("source.el"));
    let input = "(progn\n  (defvar first nil)\n\n  (defun second ()\n    nil))\n";
    fs::write(&file, input).expect("write fixture");

    paredit()
        .arg("edit")
        .arg("format")
        .arg("--dialect")
        .arg("emacs-lisp")
        .arg("--file")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::eq(input));
}

#[test]
fn cli_format_emacs_lisp_preserves_blank_lines_in_let_body() {
    let dir = fresh_temp_dir("format-elisp-let-blank-lines");
    let file = dir.join(Path::new("source.el"));
    let input = "(let ((value 1))\n  (message \"first\")\n\n  (message \"second\"))\n";
    fs::write(&file, input).expect("write fixture");

    paredit()
        .arg("edit")
        .arg("format")
        .arg("--dialect")
        .arg("emacs-lisp")
        .arg("--file")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::eq(input));
}

#[test]
fn cli_format_emacs_lisp_preserves_blank_lines_in_when_body() {
    let dir = fresh_temp_dir("format-elisp-when-blank-lines");
    let file = dir.join(Path::new("source.el"));
    let input = "(when ready\n  (message \"first\")\n\n  (message \"second\"))\n";
    fs::write(&file, input).expect("write fixture");

    paredit()
        .arg("edit")
        .arg("format")
        .arg("--dialect")
        .arg("emacs-lisp")
        .arg("--file")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::eq(input));
}

#[test]
fn cli_format_emacs_lisp_preserves_blank_lines_in_definitions() {
    let dir = fresh_temp_dir("format-elisp-definition-blank-lines");
    let file = dir.join(Path::new("source.el"));
    let input = "(defvar example-value\n  (list 'one\n        'two)\n\n  \"Example value.\")\n\n(defun example-run (value)\n  \"Process VALUE.\"\n\n  (message \"first\")\n\n  (identity value))\n";
    let expected = "(defvar example-value (list 'one 'two)\n\n  \"Example value.\")\n\n(defun example-run (value)\n  \"Process VALUE.\"\n\n  (message \"first\")\n\n  (identity value))\n";
    fs::write(&file, input).expect("write fixture");

    paredit()
        .arg("edit")
        .arg("format")
        .arg("--dialect")
        .arg("emacs-lisp")
        .arg("--file")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::eq(expected));
}

#[test]
fn cli_format_preserves_blank_lines_in_local_callable_forms() {
    let dir = fresh_temp_dir("format-local-callable-blank-lines");
    let file = dir.join(Path::new("source.lisp"));
    let input = "(macrolet ((with-a (x)\n             (prepare x)\n\n             (finish x))\n\n           (with-b (y)\n             (finish y)))\n  (with-a 1)\n\n  (with-b 2))\n\n(labels ((parse (x)\n           (validate x)\n\n           (build x))\n\n         (emit (y)\n           (write y)))\n  (parse input)\n\n  (emit output))\n";
    fs::write(&file, input).expect("write fixture");

    paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .arg("--max-blank-lines")
        .arg("1")
        .assert()
        .success()
        .stdout(predicate::eq(input));
}

#[test]
fn cli_format_emacs_lisp_corpus_fixture_is_idempotent() {
    let dir = fresh_temp_dir("format-elisp-corpus-idempotent");
    let file = dir.join(Path::new("elisp.el"));
    let input = include_str!("../../fixtures/corpus/elisp.el");
    fs::write(&file, input).expect("write fixture");

    paredit()
        .arg("edit")
        .arg("format")
        .arg("--dialect")
        .arg("emacs-lisp")
        .arg("--file")
        .arg(&file)
        .arg("--write")
        .assert()
        .success();

    let formatted = fs::read_to_string(&file).expect("read formatted fixture");
    paredit()
        .arg("edit")
        .arg("format")
        .arg("--dialect")
        .arg("emacs-lisp")
        .arg("--file")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::eq(formatted));
}

#[test]
fn cli_format_max_blank_lines_flag_preserves_blank_lines_up_to_the_maximum() {
    let dir = fresh_temp_dir("format-max-blank-lines-flag");
    let file = dir.join(Path::new("source.lisp"));
    fs::write(&file, "(a)\n\n\n(b)\n").expect("write fixture");

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
    let input = "(a)\n\n\n(b)\n";
    fs::write(root.join("source.lisp"), input).expect("write fixture");

    paredit()
        .current_dir(&root)
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg("source.lisp")
        .assert()
        .success()
        .stdout(predicate::eq("(a)\n\n(b)\n"));

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

#[test]
fn cli_format_emacs_lisp_explicit_blank_line_limits_override_default() {
    let root = repo("format-elisp-explicit-max-blank-lines");
    let input = "(a)\n\n\n(b)\n";
    fs::write(root.join("source.el"), input).expect("write fixture");
    fs::write(
        root.join("paredit.toml"),
        "[format]\nblank-lines-max-consecutive = 2\n",
    )
    .expect("write config");

    paredit()
        .current_dir(&root)
        .arg("edit")
        .arg("format")
        .arg("--dialect")
        .arg("emacs-lisp")
        .arg("--file")
        .arg("source.el")
        .assert()
        .success()
        .stdout(predicate::eq(input));

    paredit()
        .current_dir(&root)
        .arg("edit")
        .arg("format")
        .arg("--dialect")
        .arg("emacs-lisp")
        .arg("--file")
        .arg("source.el")
        .arg("--max-blank-lines")
        .arg("0")
        .assert()
        .success()
        .stdout(predicate::eq("(a)\n(b)\n"));
}
