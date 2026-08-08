//! Phase 4+5 of Section J: FR-012 (`format.numeric-literal-case` /
//! `--numeric-literal-case`), FR-013 (`format.align-clause-values` /
//! `--align-clause-values`), FR-014 (terminal-width auto-detection for
//! `--max-width`'s default — only the non-terminal branch is directly
//! testable, see below), and FR-015 (`.editorconfig` discovery), exercised
//! through the real binary.

use super::*;

/// A repository-shaped scratch directory: a `.git` marker so configuration
/// discovery has a root to stop at, mirroring
/// `tests/cli/format/indent_table_and_quote_style.rs`.
fn repo(name: &str) -> PathBuf {
    let root = fresh_temp_dir(name);
    fs::create_dir_all(root.join(".git")).expect("create .git marker");
    root
}

// --- FR-012: `format.numeric-literal-case` / `--numeric-literal-case` ---

#[test]
fn cli_format_numeric_literal_case_flag_lowercases_markers() {
    let dir = fresh_temp_dir("format-numeric-literal-case-lower");
    let file = dir.join(Path::new("source.lisp"));
    fs::write(&file, "(list #X1F 1.0D0)\n").expect("write fixture");

    // Off by default: byte-identical to before this phase.
    paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::eq("(list #X1F 1.0D0)\n"));

    // Only the marker letter recases (`X` and `D`); the hex digit `F` is
    // left exactly as written, since a hex digit's own case is never this
    // option's concern.
    paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .arg("--numeric-literal-case")
        .arg("lower")
        .assert()
        .success()
        .stdout(predicate::eq("(list #x1F 1.0d0)\n"));
}

#[test]
fn cli_format_numeric_literal_case_flag_uppercases_markers() {
    let dir = fresh_temp_dir("format-numeric-literal-case-upper");
    let file = dir.join(Path::new("source.lisp"));
    fs::write(&file, "(list #x1f 1.0e10)\n").expect("write fixture");

    paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .arg("--numeric-literal-case")
        .arg("upper")
        .assert()
        .success()
        .stdout(predicate::eq("(list #X1f 1.0E10)\n"));
}

#[test]
fn cli_format_numeric_literal_case_never_touches_a_non_numeric_symbol() {
    let dir = fresh_temp_dir("format-numeric-literal-case-symbol");
    let file = dir.join(Path::new("source.lisp"));
    fs::write(&file, "(list x1e5 not-a-number)\n").expect("write fixture");

    paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .arg("--numeric-literal-case")
        .arg("upper")
        .assert()
        .success()
        .stdout(predicate::eq("(list x1e5 not-a-number)\n"));
}

#[test]
fn cli_format_numeric_literal_case_config_key_reaches_the_command() {
    let root = repo("format-config-numeric-literal-case");
    fs::write(root.join("source.lisp"), "(list #x1f)\n").expect("write fixture");

    paredit()
        .current_dir(&root)
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg("source.lisp")
        .assert()
        .success()
        .stdout(predicate::eq("(list #x1f)\n"));

    fs::write(
        root.join("paredit.toml"),
        "[format]\nnumeric-literal-case = \"upper\"\n",
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
        .stdout(predicate::eq("(list #X1f)\n"));
}

// --- FR-013: `format.align-clause-values` / `--align-clause-values` ---

#[test]
fn cli_format_align_clause_values_flag_pads_values_to_a_shared_column() {
    let dir = fresh_temp_dir("format-align-clause-values-flag");
    let file = dir.join(Path::new("source.lisp"));
    fs::write(
        &file,
        "(let ((short-name 1) (longer-name 2)) (list short-name longer-name))\n",
    )
    .expect("write fixture");

    // Off by default: byte-identical to before this phase.
    paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::eq(
            "(let ((short-name 1)\n      (longer-name 2))\n  (list short-name longer-name))\n",
        ));

    paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .arg("--align-clause-values")
        .assert()
        .success()
        .stdout(predicate::eq(
            "(let ((short-name  1)\n      (longer-name 2))\n  (list short-name longer-name))\n",
        ));
}

#[test]
fn cli_format_align_clause_values_config_key_reaches_the_command() {
    let root = repo("format-config-align-clause-values");
    fs::write(
        root.join("source.lisp"),
        "(let ((short-name 1) (longer-name 2)) (list short-name longer-name))\n",
    )
    .expect("write fixture");

    fs::write(
        root.join("paredit.toml"),
        "[format]\nalign-clause-values = true\n",
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
        .stdout(predicate::eq(
            "(let ((short-name  1)\n      (longer-name 2))\n  (list short-name longer-name))\n",
        ));
}

// --- FR-014: terminal-width auto-detection for `--max-width`'s default ---

/// `assert_cmd`'s process always has its stdout captured (piped), never a
/// live tty, so this is the one branch of FR-014 a test harness can exercise
/// directly: piped/CI output must keep using the fixed 80-column default
/// exactly as before this option existed, regardless of the `COLUMNS`
/// environment variable a real terminal would otherwise be sized from. The
/// "stdout really is an interactive terminal, so use its width" branch has
/// no equivalent here — there is no way to give a subprocess a live tty from
/// this harness — and is verified by hand instead (documented in the
/// implementation's own doc comment on `auto_detected_terminal_width`).
#[test]
fn cli_format_max_width_default_is_unaffected_by_columns_when_piped() {
    let input = "(some-function-name argument-one argument-two argument-three argument-four argument-five)\n";
    let dir = fresh_temp_dir("format-max-width-piped-default");
    let file = dir.join(Path::new("source.lisp"));
    fs::write(&file, input).expect("write fixture");

    // A wide `COLUMNS` would, if honoured, let this form fit inline; piped
    // stdout must still wrap it at the compiled-in 80-column default.
    paredit()
        .env("COLUMNS", "500")
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::eq(input).not());
}

#[test]
fn cli_format_write_never_auto_detects_terminal_width() {
    // `--write` writes to the file, not a terminal, so it must behave
    // identically regardless of `COLUMNS` — this is directly testable
    // (unlike the interactive-terminal branch itself) because it only
    // requires proving the auto-detection path is *not* taken.
    let input = "(some-function-name argument-one argument-two argument-three argument-four argument-five)\n";
    let dir = fresh_temp_dir("format-write-no-auto-width");
    let file = dir.join(Path::new("source.lisp"));
    fs::write(&file, input).expect("write fixture");

    paredit()
        .env("COLUMNS", "500")
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .arg("--write")
        .assert()
        .success();
    assert_ne!(fs::read_to_string(&file).expect("read fixture"), input);
}

// --- FR-015: `.editorconfig` discovery ---

#[test]
fn cli_format_editorconfig_indent_size_reaches_the_command() {
    let root = repo("format-editorconfig-indent-size");
    fs::write(root.join("source.lisp"), "(if a b c)\n").expect("write fixture");
    fs::write(
        root.join(".editorconfig"),
        "root = true\n[*]\nindent_size = 4\n",
    )
    .expect("write .editorconfig");

    paredit()
        .current_dir(&root)
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg("source.lisp")
        .assert()
        .success()
        .stdout(predicate::eq("(if a\n        b\n        c)\n"));
}

#[test]
fn cli_format_editorconfig_max_line_length_reaches_the_command() {
    let root = repo("format-editorconfig-max-line-length");
    let input = "(foo (bar 1 2) (baz 3 4))\n";
    fs::write(root.join("source.lisp"), input).expect("write fixture");
    fs::write(
        root.join(".editorconfig"),
        "root = true\n[*]\nmax_line_length = 10\n",
    )
    .expect("write .editorconfig");

    paredit()
        .current_dir(&root)
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg("source.lisp")
        .assert()
        .success()
        .stdout(predicate::eq(input).not());
}

#[test]
fn cli_format_paredit_toml_wins_over_editorconfig_on_the_same_key() {
    let root = repo("format-editorconfig-precedence");
    fs::write(root.join("source.lisp"), "(if a b c)\n").expect("write fixture");
    fs::write(
        root.join(".editorconfig"),
        "root = true\n[*]\nindent_size = 4\n",
    )
    .expect("write .editorconfig");
    fs::write(root.join("paredit.toml"), "[format]\nindent = 8\n").expect("write config");

    paredit()
        .current_dir(&root)
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg("source.lisp")
        .assert()
        .success()
        .stdout(predicate::eq(
            "(if a\n                b\n                c)\n",
        ));
}

#[test]
fn cli_format_with_no_editorconfig_behaves_exactly_as_before() {
    let root = repo("format-no-editorconfig");
    fs::write(root.join("source.lisp"), "(if a b c)\n").expect("write fixture");

    paredit()
        .current_dir(&root)
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg("source.lisp")
        .assert()
        .success()
        .stdout(predicate::eq("(if a\n    b\n    c)\n"));
}

// --- FR-015: `format.insert-final-newline` / `format.trim-trailing-whitespace` ---

#[test]
fn cli_format_insert_final_newline_false_omits_the_trailing_newline() {
    let dir = fresh_temp_dir("format-insert-final-newline-flag");
    let file = dir.join(Path::new("source.lisp"));
    fs::write(&file, "(foo)\n").expect("write fixture");

    paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .arg("--insert-final-newline")
        .arg("false")
        .assert()
        .success()
        .stdout(predicate::eq("(foo)"));
}

#[test]
fn cli_format_trim_trailing_whitespace_false_keeps_a_comments_trailing_spaces() {
    let dir = fresh_temp_dir("format-trim-trailing-whitespace-flag");
    let file = dir.join(Path::new("source.lisp"));
    fs::write(&file, "(foo) ; trailing   \n(bar)\n").expect("write fixture");

    paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .arg("--trim-trailing-whitespace")
        .arg("false")
        .assert()
        .success()
        .stdout(predicate::str::contains("trailing   \n"));
}
