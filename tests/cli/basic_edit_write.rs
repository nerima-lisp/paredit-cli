use super::*;

#[test]
fn edit_wrap_write_updates_file_in_place_and_prints_nothing() {
    let dir = fresh_temp_dir("edit-wrap-write");
    let file = dir.join("source.lisp");
    fs::write(&file, "(defun foo (x) (+ x 1))\n").expect("write source fixture");

    paredit()
        .args(["edit", "wrap", "--path", "0.2", "--write", "--file"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::is_empty());

    let rewritten = fs::read_to_string(&file).expect("read rewritten source");
    assert_eq!(rewritten, "(defun foo ((x)) (+ x 1))\n");
}

#[test]
fn edit_replace_write_updates_file_in_place() {
    let dir = fresh_temp_dir("edit-replace-write");
    let file = dir.join("source.lisp");
    fs::write(&file, "(defun foo (x) (+ x 1))\n").expect("write source fixture");

    paredit()
        .args([
            "edit", "replace", "--path", "0.3", "--with", "(- x 1)", "--write", "--file",
        ])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::is_empty());

    let rewritten = fs::read_to_string(&file).expect("read rewritten source");
    assert_eq!(rewritten, "(defun foo (x) (- x 1))\n");
}

#[test]
fn edit_replace_trims_trailing_whitespace_only_from_the_changed_line() {
    let dir = fresh_temp_dir("edit-replace-local-trailing-whitespace");
    let file = dir.join("source.lisp");
    fs::write(&file, "(defun foo (x) (+ x 1)) \n(keep) \n").expect("write source fixture");

    paredit()
        .args([
            "edit", "replace", "--path", "0.3", "--with", "(- x 1)", "--write", "--file",
        ])
        .arg(&file)
        .assert()
        .success();

    let rewritten = fs::read_to_string(&file).expect("read rewritten source");
    assert_eq!(rewritten, "(defun foo (x) (- x 1))\n(keep) \n");
}

#[test]
fn edit_format_write_updates_file_in_place() {
    let dir = fresh_temp_dir("edit-format-write");
    let file = dir.join("source.lisp");
    fs::write(&file, "(defun foo (x)\n(+ x 1))\n").expect("write source fixture");

    paredit()
        .args(["edit", "format", "--write", "--file"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::is_empty());

    let rewritten = fs::read_to_string(&file).expect("read rewritten source");
    assert!(rewritten.contains("defun foo"), "{rewritten}");
    paredit()
        .args(["inspect", "check", "--file"])
        .arg(&file)
        .assert()
        .success();
}

#[test]
fn edit_write_requires_file_input() {
    paredit()
        .args(["edit", "kill", "--path", "0", "--write"])
        .write_stdin("(a b)")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--write requires --file"));
}

#[test]
fn edit_diff_prints_unified_diff_and_keeps_file_untouched() {
    let dir = fresh_temp_dir("edit-wrap-diff");
    let file = dir.join("source.lisp");
    let original = "(defun foo (x) (+ x 1))\n";
    fs::write(&file, original).expect("write source fixture");

    paredit()
        .args(["edit", "wrap", "--path", "0.2", "--diff", "--file"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("-(defun foo (x) (+ x 1))"))
        .stdout(predicate::str::contains("+(defun foo ((x)) (+ x 1))"));

    let untouched = fs::read_to_string(&file).expect("read source");
    assert_eq!(untouched, original);
}

#[test]
fn edit_diff_with_write_updates_file_and_prints_diff() {
    let dir = fresh_temp_dir("edit-wrap-diff-write");
    let file = dir.join("source.lisp");
    fs::write(&file, "(defun foo (x) (+ x 1))\n").expect("write source fixture");

    paredit()
        .args([
            "edit", "wrap", "--path", "0.2", "--diff", "--write", "--file",
        ])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("+(defun foo ((x)) (+ x 1))"));

    let rewritten = fs::read_to_string(&file).expect("read rewritten source");
    assert_eq!(rewritten, "(defun foo ((x)) (+ x 1))\n");
}

#[test]
fn edit_without_write_prints_to_stdout_and_keeps_file_untouched() {
    let dir = fresh_temp_dir("edit-kill-stdout");
    let file = dir.join("source.lisp");
    let original = "(a b)\n(c d)\n";
    fs::write(&file, original).expect("write source fixture");

    paredit()
        .args(["edit", "kill", "--path", "1", "--file"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("(a b)"));

    let untouched = fs::read_to_string(&file).expect("read source");
    assert_eq!(untouched, original);
}

fn assert_edit_output(subcommand: &str, path: &str, expected: &str) {
    paredit()
        .args(["edit", subcommand, "--path", path])
        .write_stdin("(a (b c) d e)\n")
        .assert()
        .success()
        .stdout(predicate::str::contains(expected));
}

#[test]
fn edit_replace_keeps_generic_stdin_compatibility_without_dialect_flag() {
    paredit()
        .args(["edit", "replace", "--path", "1", "--with", "(new)"])
        .write_stdin("(old) (keep)\n")
        .assert()
        .success()
        .stdout(predicate::eq("(old) (new)\n"));
}

#[test]
fn edit_replace_uses_explicit_clojure_dialect_for_stdin_reader_forms() {
    paredit()
        .args([
            "edit",
            "replace",
            "--dialect",
            "clojure",
            "--path",
            "1",
            "--with",
            "(new)",
        ])
        .write_stdin("#inst \"1985-04-12T23:20:50.52-00:00\" (old)\n")
        .assert()
        .success()
        .stdout(predicate::eq(
            "#inst \"1985-04-12T23:20:50.52-00:00\" (new)\n",
        ));
}

#[test]
fn edit_replace_uses_explicit_common_lisp_dialect_for_stdin_reader_forms() {
    paredit()
        .args([
            "edit",
            "replace",
            "--dialect",
            "common-lisp",
            "--path",
            "1",
            "--with",
            "(new)",
        ])
        .write_stdin("#S(point :x 1) (old)\n")
        .assert()
        .success()
        .stdout(predicate::eq("#S(point :x 1) (new)\n"));
}

#[test]
fn edit_splice_removes_one_list_pair() {
    assert_edit_output("splice", "0.1", "(a b c d e)");
}

#[test]
fn edit_raise_replaces_parent_with_selection() {
    assert_edit_output("raise", "0.1.0", "(a b d e)");
}

#[test]
fn edit_slurp_forward_pulls_next_sibling_in() {
    assert_edit_output("slurp-forward", "0.1", "(a (b c d) e)");
}

#[test]
fn edit_slurp_backward_pulls_previous_sibling_in() {
    assert_edit_output("slurp-backward", "0.1", "((a b c) d e)");
}

#[test]
fn edit_barf_forward_pushes_last_child_out() {
    assert_edit_output("barf-forward", "0.1", "(a (b) c d e)");
}

#[test]
fn edit_barf_backward_pushes_first_child_out() {
    assert_edit_output("barf-backward", "0.1", "(a b (c) d e)");
}

#[test]
fn edit_wrap_uses_requested_square_bracket_delimiter() {
    paredit()
        .args(["edit", "wrap", "--path", "0.1", "--delimiter", "bracket"])
        .write_stdin("(alpha beta)\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("(alpha [beta])"));
}

#[test]
fn edit_wrap_write_defaults_to_parentheses() {
    let dir = fresh_temp_dir("edit-wrap-default-delimiter");
    let file = dir.join("source.lisp");
    fs::write(&file, "(alpha beta)\n").expect("write source fixture");

    paredit()
        .args(["edit", "wrap", "--path", "0.1", "--write", "--file"])
        .arg(&file)
        .assert()
        .success();

    let rewritten = fs::read_to_string(&file).expect("read rewritten source");
    assert_eq!(rewritten, "(alpha (beta))\n");
}

#[test]
fn edit_split_divides_enclosing_list_before_selection() {
    assert_edit_output("split", "0.2", "(a (b c)) (d e)");
}

#[test]
fn edit_join_merges_selected_list_with_next_sibling() {
    paredit()
        .args(["edit", "join", "--path", "0"])
        .write_stdin("(a b) (c d)\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("(a b c d)"));
}

#[test]
fn edit_split_write_updates_file_in_place() {
    let dir = fresh_temp_dir("edit-split-write");
    let file = dir.join("source.lisp");
    fs::write(&file, "(list one two three)\n").expect("write source fixture");

    paredit()
        .args(["edit", "split", "--path", "0.2", "--write", "--file"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::is_empty());

    let rewritten = fs::read_to_string(&file).expect("read rewritten source");
    assert_eq!(rewritten, "(list one) (two three)\n");
}

#[test]
fn edit_join_write_updates_file_in_place() {
    let dir = fresh_temp_dir("edit-join-write");
    let file = dir.join("source.lisp");
    fs::write(&file, "(list one) (two three)\n").expect("write source fixture");

    paredit()
        .args(["edit", "join", "--path", "0", "--write", "--file"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::is_empty());

    let rewritten = fs::read_to_string(&file).expect("read rewritten source");
    assert_eq!(rewritten, "(list one two three)\n");
}

#[test]
fn edit_join_merges_adjacent_string_literals() {
    paredit()
        .args(["edit", "join", "--path", "0.1"])
        .write_stdin("(concat \"foo\" \"bar\")\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("(concat \"foobar\")"));
}

#[test]
fn edit_splice_killing_backward_keeps_selection_and_tail() {
    paredit()
        .args(["edit", "splice-killing-backward", "--path", "0.2"])
        .write_stdin("(let ((x 5)) (foo x) bar)\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("(foo x) bar"));
}

#[test]
fn edit_splice_killing_forward_keeps_preceding_siblings() {
    paredit()
        .args(["edit", "splice-killing-forward", "--path", "0.2"])
        .write_stdin("(foo (bar) baz qux)\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("foo (bar)"));
}

#[test]
fn edit_convolute_reverses_two_enclosing_lists() {
    paredit()
        .args(["edit", "convolute", "--path", "0.2.1"])
        .write_stdin("(let ((x 1)) (foo (bar baz) quux))\n")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "(foo (let ((x 1)) (bar baz)) quux)",
        ));
}

#[test]
fn edit_convolute_write_updates_file_in_place() {
    let dir = fresh_temp_dir("edit-convolute-write");
    let file = dir.join("source.lisp");
    fs::write(&file, "(let ((x 1)) (foo (bar baz) quux))\n").expect("write source fixture");

    paredit()
        .args(["edit", "convolute", "--path", "0.2.1", "--write", "--file"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::is_empty());

    let rewritten = fs::read_to_string(&file).expect("read rewritten source");
    assert_eq!(rewritten, "(foo (let ((x 1)) (bar baz)) quux)\n");
}
