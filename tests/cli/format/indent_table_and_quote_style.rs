//! CLI coverage for indentation tables, width profiles, and quote styles,
//! including command-line flags and `paredit.toml` keys.

use super::*;

/// A repository-shaped scratch directory: a `.git` marker so configuration
/// discovery has a root to stop at, mirroring
/// `tests/cli/format/config_and_multi_file.rs`.
fn repo(name: &str) -> PathBuf {
    let root = fresh_temp_dir(name);
    fs::create_dir_all(root.join(".git")).expect("create .git marker");
    root
}

#[test]
fn cli_format_indent_table_flag_retargets_a_symbol_onto_a_style() {
    let dir = fresh_temp_dir("format-indent-table-flag");
    let file = dir.join(Path::new("source.lisp"));
    fs::write(&file, "(if a b c)\n").expect("write fixture");

    // `if` never inlines by default (`ListStyle::IfAligned` always shows
    // its structure with all branches at the same distinguished column).
    paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::eq("(if a\n    b\n    c)\n"));

    // `--indent-table if=general` retargets `if` onto the plain-call layout,
    // so a short `if` form inlines like any other call.
    paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .arg("--indent-table")
        .arg("if=general")
        .assert()
        .success()
        .stdout(predicate::eq("(if a b c)\n"));
}

#[test]
fn cli_format_indent_table_config_key_reaches_the_command() {
    let root = repo("format-config-indent-table");
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

    fs::write(
        root.join("paredit.toml"),
        "[format]\nindent-table = [\"if=general\"]\n",
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
        .stdout(predicate::eq("(if a b c)\n"));
}

#[test]
fn cli_format_indent_table_flag_with_a_duplicate_symbol_uses_the_last_entry() {
    // Two `--indent-table` entries for the same symbol on one invocation: the
    // last one must win, matching `Formatter::with_indent_overrides`'s field
    // doc ("later entries... win over earlier ones"). `general` first, then
    // `if-then-else` last — the output must show `if-then-else`'s layout,
    // not `general`'s inline one.
    let dir = fresh_temp_dir("format-indent-table-duplicate-symbol");
    let file = dir.join(Path::new("source.lisp"));
    fs::write(&file, "(frob a b c)\n").expect("write fixture");

    paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .arg("--indent-table")
        .arg("frob=general")
        .arg("--indent-table")
        .arg("frob=if-then-else")
        .assert()
        .success()
        .stdout(predicate::eq("(frob a b\n  c)\n"));
}

#[test]
fn cli_format_indent_table_flag_rejects_an_unrecognised_style_name() {
    let dir = fresh_temp_dir("format-indent-table-unknown-style");
    let file = dir.join(Path::new("source.lisp"));
    fs::write(&file, "(if a b c)\n").expect("write fixture");

    paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .arg("--indent-table")
        .arg("if=not-a-real-style")
        .assert()
        .failure()
        .stderr(predicate::str::contains("not-a-real-style"));
}

#[test]
fn cli_format_indent_table_flag_rejects_a_malformed_entry() {
    let dir = fresh_temp_dir("format-indent-table-malformed");
    let file = dir.join(Path::new("source.lisp"));
    fs::write(&file, "(if a b c)\n").expect("write fixture");

    paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .arg("--indent-table")
        .arg("if")
        .assert()
        .failure()
        .stderr(predicate::str::contains("SYMBOL=STYLE"));
}

#[test]
fn cli_format_width_profile_flag_narrows_general_forms() {
    let dir = fresh_temp_dir("format-width-profile-flag");
    let file = dir.join(Path::new("source.lisp"));
    let input = "(foo (bar 1 2) (baz 3 4))\n";
    fs::write(&file, input).expect("write fixture");

    // Fits inline at the compiled-in default.
    paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::eq(input));

    // A narrow `general` profile stops it fitting, without touching the
    // global `--max-width`.
    paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .arg("--width-profile")
        .arg("general=10")
        .assert()
        .success()
        .stdout(predicate::eq(input).not());
}

#[test]
fn cli_format_width_profile_flag_with_a_duplicate_style_uses_the_last_entry() {
    // Same duplicate-entry precedence check, for `--width-profile`: `5`
    // first, then `200` last. The last entry must win, so the form must fit
    // inline despite the earlier narrow entry.
    let dir = fresh_temp_dir("format-width-profile-duplicate-style");
    let file = dir.join(Path::new("source.lisp"));
    let input = "(foo (bar 1 2) (baz 3 4))\n";
    fs::write(&file, input).expect("write fixture");

    paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .arg("--width-profile")
        .arg("general=5")
        .arg("--width-profile")
        .arg("general=200")
        .assert()
        .success()
        .stdout(predicate::eq(input));
}

#[test]
fn cli_format_width_profiles_config_key_reaches_the_command() {
    let root = repo("format-config-width-profiles");
    let input = "(foo (bar 1 2) (baz 3 4))\n";
    fs::write(root.join("source.lisp"), input).expect("write fixture");

    paredit()
        .current_dir(&root)
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg("source.lisp")
        .assert()
        .success()
        .stdout(predicate::eq(input));

    fs::write(
        root.join("paredit.toml"),
        "[format]\nwidth-profiles = [\"general=10\"]\n",
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
        .stdout(predicate::eq(input).not());
}

#[test]
fn cli_format_width_profile_flag_rejects_a_non_integer_width() {
    let dir = fresh_temp_dir("format-width-profile-bad-width");
    let file = dir.join(Path::new("source.lisp"));
    fs::write(&file, "(foo bar)\n").expect("write fixture");

    paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .arg("--width-profile")
        .arg("general=wide")
        .assert()
        .failure()
        .stderr(predicate::str::contains("integer"));
}

#[test]
fn cli_format_quote_style_flag_expands_a_quote_prefix() {
    let dir = fresh_temp_dir("format-quote-style-flag");
    let file = dir.join(Path::new("source.lisp"));
    fs::write(&file, "'(alpha beta)\n").expect("write fixture");

    // The default leaves the source byte-identical.
    paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::eq("'(alpha beta)\n"));

    paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .arg("--quote-style")
        .arg("canonical")
        .assert()
        .success()
        .stdout(predicate::eq("(quote (alpha beta))\n"));
}

#[test]
fn cli_format_quote_style_config_key_reaches_the_command() {
    let root = repo("format-config-quote-style");
    fs::write(root.join("source.lisp"), "'(alpha beta)\n").expect("write fixture");

    paredit()
        .current_dir(&root)
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg("source.lisp")
        .assert()
        .success()
        .stdout(predicate::eq("'(alpha beta)\n"));

    fs::write(
        root.join("paredit.toml"),
        "[format]\nquote-style = \"canonical\"\n",
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
        .stdout(predicate::eq("(quote (alpha beta))\n"));
}

#[test]
fn cli_format_quote_style_canonical_never_touches_quasiquote_or_unquote() {
    let dir = fresh_temp_dir("format-quote-style-quasiquote");
    let file = dir.join(Path::new("source.lisp"));
    let input = "`(list ,item ,@rest)\n";
    fs::write(&file, input).expect("write fixture");

    paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .arg("--quote-style")
        .arg("canonical")
        .assert()
        .success()
        .stdout(predicate::eq(input));
}
