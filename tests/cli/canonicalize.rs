//! `edit canonicalize`, exercised through the real binary.
//!
//! Every test here targets one of the corruption classes past write-path
//! incidents in this codebase were caused by: CRLF mangled to LF, a string's
//! escapes or embedded newline changed on the way through, a reader-prefixed
//! value silently dropped, a quoted form re-sorted as if it were evaluated
//! data, or a numeric literal reprinted in a different notation. Every
//! assertion checks the exact output text, not merely that it still parses.

use super::*;

/// Corruption class 6: a comment must never be silently dropped. This
/// command reorders and reflows entries, and has no way to keep a comment
/// attached to the entry it belongs to, so — rather than lose it — the whole
/// operation is refused up front whenever the tree carries any comment at
/// all, on both the `--write` and the stdout-only path.
#[test]
fn a_leading_standalone_comment_is_refused_rather_than_dropped_on_write() {
    let dir = fresh_temp_dir("canonicalize-comment-leading-write");
    let file = dir.join("data.lisp");
    let source = ";; keep this comment\n(:b 2 :a 1)\n";
    fs::write(&file, source).expect("write fixture");

    paredit()
        .args(["edit", "canonicalize", "--write", "--file"])
        .arg(&file)
        .assert()
        .failure()
        .stderr(predicate::str::contains("containing a comment"));

    assert_eq!(
        fs::read_to_string(&file).expect("read untouched fixture"),
        source
    );
}

#[test]
fn a_leading_standalone_comment_is_refused_on_the_stdout_only_path() {
    paredit()
        .args(["edit", "canonicalize"])
        .write_stdin(";; keep this comment\n(:b 2 :a 1)\n")
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("containing a comment"));
}

#[test]
fn an_inline_trailing_comment_is_refused_rather_than_dropped_on_write() {
    let dir = fresh_temp_dir("canonicalize-comment-trailing-write");
    let file = dir.join("data.lisp");
    let source = "(:b 2 ; trailing note on b\n :a 1)\n";
    fs::write(&file, source).expect("write fixture");

    paredit()
        .args(["edit", "canonicalize", "--write", "--file"])
        .arg(&file)
        .assert()
        .failure()
        .stderr(predicate::str::contains("containing a comment"));

    assert_eq!(
        fs::read_to_string(&file).expect("read untouched fixture"),
        source
    );
}

#[test]
fn an_inline_trailing_comment_is_refused_on_the_stdout_only_path() {
    paredit()
        .args(["edit", "canonicalize"])
        .write_stdin("(:b 2 ; trailing note on b\n :a 1)\n")
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("containing a comment"));
}

/// Corruption class 7: a quasiquoted subtree is a reader-prefixed value like
/// `'quoted` — its own internal order (and its unquote/unquote-splicing
/// forms) is the author's choice, not alist/plist data to re-sort, and it
/// must be copied whole rather than descended into. The plist *around* it is
/// still reordered.
#[test]
fn a_quasiquoted_forms_internal_order_is_left_alone_while_siblings_sort() {
    let source = "(:z 3 :y \"y\" :a `(:z 1 ,x))";
    let expected = "(:a `(:z 1 ,x) :y \"y\" :z 3)\n";

    let dir = fresh_temp_dir("canonicalize-quasiquote");
    let file = dir.join("data.lisp");
    fs::write(&file, source).expect("write fixture");

    paredit()
        .args(["edit", "canonicalize", "--write", "--file"])
        .arg(&file)
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(&file).expect("read canonicalized fixture"),
        expected
    );
}

#[test]
fn a_quasiquoted_forms_unquote_splicing_is_left_alone_while_siblings_sort() {
    let source = "(:z 3 :y \"y\" :a `(:z 1 ,@xs))";
    let expected = "(:a `(:z 1 ,@xs) :y \"y\" :z 3)\n";

    let dir = fresh_temp_dir("canonicalize-quasiquote-splicing");
    let file = dir.join("data.lisp");
    fs::write(&file, source).expect("write fixture");

    paredit()
        .args(["edit", "canonicalize", "--write", "--file"])
        .arg(&file)
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(&file).expect("read canonicalized fixture"),
        expected
    );
}

/// Corruption class 8: malformed source on a write path must fail cleanly —
/// no panic, a non-zero exit, a clear error, and the file left completely
/// untouched — rather than doing anything undefined.
#[test]
fn unbalanced_parens_are_refused_cleanly_and_the_file_is_left_untouched() {
    let dir = fresh_temp_dir("canonicalize-unbalanced");
    let file = dir.join("data.lisp");
    let source = "(:a 1";
    fs::write(&file, source).expect("write fixture");

    paredit()
        .args(["edit", "canonicalize", "--write", "--file"])
        .arg(&file)
        .assert()
        .failure()
        .stderr(predicate::str::contains("unclosed list"));

    assert_eq!(
        fs::read_to_string(&file).expect("read untouched fixture"),
        source
    );
}

#[test]
fn a_plist_is_sorted_and_rewhitespaced_when_written() {
    let dir = fresh_temp_dir("canonicalize-plist");
    let file = dir.join("data.lisp");
    fs::write(&file, "(:c   3\n  :a 1  :b 2)").expect("write fixture");

    paredit()
        .args(["edit", "canonicalize", "--write", "--file"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::is_empty());

    assert_eq!(
        fs::read_to_string(&file).expect("read canonicalized fixture"),
        "(:a 1 :b 2 :c 3)\n"
    );
}

#[test]
fn a_file_with_no_alist_or_plist_shape_is_refused() {
    paredit()
        .args(["edit", "canonicalize"])
        .write_stdin("(defun add (a b) (+ a b))")
        .assert()
        .failure()
        .stderr(predicate::str::contains("not confidently data"));
}

/// Corruption class 1: a CRLF file's line endings must survive, not get
/// silently downgraded to LF the way a whole-document rewrite can.
#[test]
fn crlf_line_endings_survive() {
    let dir = fresh_temp_dir("canonicalize-crlf");
    let file = dir.join("data.lisp");
    fs::write(&file, "(:b 2\r\n :a 1)\r\n").expect("write fixture");

    paredit()
        .args(["edit", "canonicalize", "--write", "--file"])
        .arg(&file)
        .assert()
        .success();

    let bytes = fs::read(&file).expect("read canonicalized fixture");
    let text = String::from_utf8(bytes).expect("utf8");
    assert_eq!(text, "(:a 1 :b 2)\r\n");
}

/// Corruption class 2: a string carrying an escaped backslash, an escaped
/// quote, and a literal embedded newline must round-trip byte for byte.
/// Only the whitespace *between* forms may change.
#[test]
fn a_strings_escapes_and_embedded_newline_round_trip_exactly() {
    let source = "(:b \"line one\nline two \\\"quoted\\\" and \\\\backslash\" :a 1)\n";
    let expected = "(:a 1 :b \"line one\nline two \\\"quoted\\\" and \\\\backslash\")\n";

    let dir = fresh_temp_dir("canonicalize-string-escapes");
    let file = dir.join("data.lisp");
    fs::write(&file, source).expect("write fixture");

    paredit()
        .args(["edit", "canonicalize", "--write", "--file"])
        .arg(&file)
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(&file).expect("read canonicalized fixture"),
        expected
    );
}

/// Corruption class 3: a value behind a reader prefix (`#+feature (form)`)
/// must be preserved intact — not silently dropped, not descended into.
#[test]
fn a_reader_prefixed_value_is_preserved_intact() {
    let source = "(:b #+sbcl (:z 2) :a 1)";
    let expected = "(:a 1 :b #+sbcl (:z 2))\n";

    let dir = fresh_temp_dir("canonicalize-reader-prefix");
    let file = dir.join("data.lisp");
    fs::write(&file, source).expect("write fixture");

    paredit()
        .args(["edit", "canonicalize", "--write", "--file"])
        .arg(&file)
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(&file).expect("read canonicalized fixture"),
        expected
    );
}

/// Corruption class 4: a quoted form's internal order is the author's
/// choice, not necessarily alist/plist data to re-sort — a quoted value is
/// copied whole, untouched down to its own irregular internal whitespace,
/// even while the plist *around* it is still reordered.
#[test]
fn a_quoted_forms_internal_order_and_whitespace_are_left_alone() {
    let source = "(:b 3 :a '(:z   1  :y 2))";
    let expected = "(:a '(:z   1  :y 2) :b 3)\n";

    let dir = fresh_temp_dir("canonicalize-quoted");
    let file = dir.join("data.lisp");
    fs::write(&file, source).expect("write fixture");

    paredit()
        .args(["edit", "canonicalize", "--write", "--file"])
        .arg(&file)
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(&file).expect("read canonicalized fixture"),
        expected
    );
}

/// Corruption class 5: a float must print back exactly — no numeric-tower
/// reinterpretation that would normalize `1.50e2` to `150.0` or drop the
/// double-float marker off `3.0d0`.
#[test]
fn a_floats_own_notation_is_preserved_exactly() {
    let source = "(:b 1.50e2 :a 3.0d0)";
    let expected = "(:a 3.0d0 :b 1.50e2)\n";

    let dir = fresh_temp_dir("canonicalize-float");
    let file = dir.join("data.lisp");
    fs::write(&file, source).expect("write fixture");

    paredit()
        .args(["edit", "canonicalize", "--write", "--file"])
        .arg(&file)
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(&file).expect("read canonicalized fixture"),
        expected
    );
}

#[test]
fn without_write_the_document_is_printed_and_the_file_is_untouched() {
    let dir = fresh_temp_dir("canonicalize-no-write");
    let file = dir.join("data.lisp");
    fs::write(&file, "(:b 2 :a 1)").expect("write fixture");

    paredit()
        .args(["edit", "canonicalize", "--file"])
        .arg(&file)
        .assert()
        .success()
        .stdout("(:a 1 :b 2)\n");

    assert_eq!(
        fs::read_to_string(&file).expect("read untouched fixture"),
        "(:b 2 :a 1)"
    );
}
