//! The `paredit.el` operations the `edit` namespace was missing, end to end.
//!
//! Each block below is one gap this section closed. The core-level tests in
//! `paredit-core-syntax` cover the rewrite rules; these cover the part only the
//! binary can prove — that the flag reaches the rule, that a refusal reaches
//! the exit code, and that the two halves of a pair (`escape`/`unescape`,
//! `split-string`/`join`, `copy`/`yank`) compose through the process boundary.

use super::*;

fn edit(args: &[&str], input: &str) -> String {
    let output = paredit()
        .args(args)
        .write_stdin(input.to_owned())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    String::from_utf8(output).expect("stdout is utf8")
}

// --- K1 / K2 / K3: wrap in a string, wrap in reader sugar, and unwrap it ---

#[test]
fn wrap_doublequote_quotes_the_selection_and_escapes_what_it_contains() {
    assert_eq!(
        edit(
            &[
                "edit",
                "wrap",
                "--path",
                "0.1",
                "--delimiter",
                "doublequote"
            ],
            "(list (a \"b\"))"
        ),
        "(list \"(a \\\"b\\\")\")"
    );
}

#[test]
fn wrap_prefix_attaches_reader_sugar_outside_what_is_already_there() {
    assert_eq!(
        edit(
            &["edit", "wrap", "--path", "0.1", "--prefix", "quote"],
            "(list #'f)"
        ),
        "(list '#'f)"
    );
}

#[test]
fn wrap_refuses_a_delimiter_and_a_prefix_at_once() {
    paredit()
        .args([
            "edit",
            "wrap",
            "--path",
            "0.1",
            "--prefix",
            "quote",
            "--delimiter",
            "bracket",
        ])
        .write_stdin("(list x)")
        .assert()
        .code(2);
}

#[test]
fn unwrap_prefix_peels_one_level_and_all_peels_every_level() {
    assert_eq!(
        edit(&["edit", "unwrap-prefix", "--path", "0.1"], "(list '#'f)"),
        "(list #'f)"
    );
    assert_eq!(
        edit(
            &["edit", "unwrap-prefix", "--path", "0.1", "--all-prefixes"],
            "(list '#'f)"
        ),
        "(list f)"
    );
}

#[test]
fn unwrap_prefix_reports_a_form_that_carries_none() {
    paredit()
        .args(["edit", "unwrap-prefix", "--path", "0.1"])
        .write_stdin("(list f)")
        .assert()
        .failure()
        .stderr(predicate::str::contains("no reader prefix"));
}

// --- K4: navigate ---

#[test]
fn navigate_prints_a_path_the_other_commands_accept() {
    let path = edit(
        &[
            "edit",
            "navigate",
            "--path",
            "0.0",
            "--direction",
            "forward",
        ],
        "(alpha beta gamma)",
    );
    assert_eq!(path, "0.1\n");
    assert_eq!(
        edit(
            &["edit", "select", "--path", path.trim()],
            "(alpha beta gamma)"
        ),
        "beta"
    );
}

#[test]
fn navigate_moves_up_and_down_the_tree() {
    assert_eq!(
        edit(
            &["edit", "navigate", "--path", "0.1", "--direction", "down"],
            "(a (b c))"
        ),
        "0.1.0\n"
    );
    assert_eq!(
        edit(
            &["edit", "navigate", "--path", "0.1.0", "--direction", "up"],
            "(a (b c))"
        ),
        "0.1\n"
    );
}

#[test]
fn navigate_json_reports_both_ends_of_the_move() {
    let report: serde_json::Value = serde_json::from_str(&edit(
        &[
            "edit",
            "navigate",
            "--path",
            "0.1",
            "--direction",
            "forward",
            "--output",
            "json",
        ],
        "(a (b c) (d e))",
    ))
    .expect("navigate emits valid JSON");
    assert_eq!(report["from"]["path"], "0.1");
    assert_eq!(report["from"]["head"], "b");
    assert_eq!(report["to"]["path"], "0.2");
    assert_eq!(report["to"]["head"], "d");
}

#[test]
fn navigate_refuses_at_the_boundary_rather_than_changing_depth() {
    paredit()
        .args([
            "edit",
            "navigate",
            "--path",
            "0.1",
            "--direction",
            "forward",
        ])
        .write_stdin("(a b) (c)")
        .assert()
        .failure()
        .stderr(predicate::str::contains("no next sibling"));
}

// --- K5: structure-safe character deletion ---

#[test]
fn delete_forward_removes_an_ordinary_character() {
    assert_eq!(
        edit(&["edit", "delete-forward", "--at", "7"], "(list abc)"),
        "(list ac)"
    );
}

#[test]
fn delete_removes_an_empty_pair_whole_from_either_side() {
    assert_eq!(
        edit(&["edit", "delete-forward", "--at", "6"], "(list ())"),
        "(list )"
    );
    assert_eq!(
        edit(&["edit", "delete-backward", "--at", "8"], "(list ())"),
        "(list )"
    );
}

#[test]
fn delete_refuses_a_delimiter_that_holds_something() {
    paredit()
        .args(["edit", "delete-forward", "--at", "0"])
        .write_stdin("(list a)")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unbalance"));
}

#[test]
fn delete_refuses_the_whitespace_holding_two_symbols_apart() {
    // Offset 7 is the space between `a` and `b`; removing it would read back
    // as the single symbol `ab`.
    paredit()
        .args(["edit", "delete-forward", "--at", "7"])
        .write_stdin("(list a b)")
        .assert()
        .failure()
        .stderr(predicate::str::contains("keeps two symbols apart"));
}

#[test]
fn delete_takes_an_escape_and_its_character_together() {
    // `(list "a\"b")` — deleting the backslash must take the quote it escapes,
    // or the string ends early and the document stops parsing.
    let source = "(list \"a\\\"b\")";
    let backslash = source.find('\\').expect("fixture has a backslash");
    assert_eq!(
        edit(
            &["edit", "delete-forward", "--at", &backslash.to_string()],
            source
        ),
        "(list \"ab\")"
    );
}

// --- K6: structure-safe newline insertion ---

#[test]
fn newline_breaks_the_line_and_reindents_the_definition() {
    assert_eq!(
        edit(
            &["edit", "newline", "--at", "13"],
            "(defun f (x) (list x))\n"
        ),
        "(defun f (x)\n  (list x))\n"
    );
}

#[test]
fn newline_without_reindent_only_inserts() {
    // `--no-reindent` skips the reindent, not the trailing-whitespace cleanup
    // every other edit in the namespace runs: the space the break stranded at
    // the end of the line is still removed.
    assert_eq!(
        edit(
            &["edit", "newline", "--at", "13", "--no-reindent"],
            "(defun f (x) (list x))\n"
        ),
        "(defun f (x)\n(list x))\n"
    );
}

#[test]
fn newline_refuses_the_inside_of_a_string_a_comment_and_a_symbol() {
    for (source, offset, context) in [
        ("(list \"ab\")", 8, "a string literal"),
        ("(list a) ; note\n", 12, "a comment"),
        ("(list abc)", 8, "a symbol"),
    ] {
        paredit()
            .args(["edit", "newline", "--at", &offset.to_string()])
            .write_stdin(source)
            .assert()
            .failure()
            .stderr(predicate::str::contains(context));
    }
}

// --- K7 / K8: copy, the kill ring, and yank ---

#[test]
fn copy_takes_the_comment_block_that_select_leaves_behind() {
    let source = "(progn\n  ;; explain\n  (f y)\n  (g z))\n";
    assert_eq!(
        edit(&["edit", "copy", "--path", "0.1"], source),
        "  ;; explain\n  (f y)"
    );
    assert_eq!(edit(&["edit", "select", "--path", "0.1"], source), "(f y)");
}

#[test]
fn copy_then_yank_moves_a_documented_form_between_files() {
    let dir = fresh_temp_dir("kill-ring");
    let ring = dir.join("ring.json");
    let source = dir.join("source.lisp");
    let target = dir.join("target.lisp");
    fs::write(&source, "(progn\n  ;; explain\n  (f y)\n  (g z))\n").expect("write source");
    fs::write(&target, "(progn\n  (h w))\n").expect("write target");

    paredit()
        .args(["edit", "copy", "--path", "0.1", "--to-ring", "--ring"])
        .arg(&ring)
        .arg("--file")
        .arg(&source)
        .assert()
        .success();

    paredit()
        .args([
            "edit",
            "yank",
            "--path",
            "0.1",
            "--placement",
            "before",
            "--write",
            "--ring",
        ])
        .arg(&ring)
        .arg("--file")
        .arg(&target)
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(&target).expect("read target"),
        "(progn\n  ;; explain\n  (f y)\n  (h w))\n"
    );
}

#[test]
fn kill_to_ring_stores_exactly_what_it_removed() {
    let dir = fresh_temp_dir("kill-ring-kill");
    let ring = dir.join("ring.json");
    let source = dir.join("source.lisp");
    fs::write(&source, "(progn\n  ;; explain\n  (f y)\n  (g z))\n").expect("write source");

    paredit()
        .args(["edit", "kill", "--path", "0.1", "--to-ring", "--ring"])
        .arg(&ring)
        .arg("--file")
        .arg(&source)
        .assert()
        .success();

    let stored: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&ring).expect("read ring")).expect("ring is JSON");
    assert_eq!(stored["entries"][0]["text"], "(f y)");
}

#[test]
fn kill_to_ring_keeps_all_and_pushes_in_source_order() {
    // `--to-ring` must not cost `kill` the selector layer's `--all`, and the
    // ring's newest entry should be the last form killed in the file.
    let dir = fresh_temp_dir("kill-ring-all");
    let ring = dir.join("ring.json");
    let source = dir.join("source.lisp");
    fs::write(
        &source,
        "(progn\n  (cleanup a)\n  (keep b)\n  (cleanup c))\n",
    )
    .expect("write");

    paredit()
        .args([
            "edit",
            "kill",
            "--dialect",
            "common-lisp",
            "--query",
            "(cleanup ?x)",
            "--all",
            "--to-ring",
            "--write",
            "--ring",
        ])
        .arg(&ring)
        .arg("--file")
        .arg(&source)
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(&source).expect("read source"),
        "(progn\n  (keep b))\n"
    );
    let stored: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&ring).expect("read ring")).expect("ring is JSON");
    assert_eq!(stored["entries"][0]["text"], "(cleanup c)");
    assert_eq!(stored["entries"][1]["text"], "(cleanup a)");
}

#[test]
fn the_new_commands_accept_the_selectors_the_selector_layer_added() {
    // `TargetArgs` carries `SelectorArgs`, so `--name` and friends reach every
    // command added here without each one restating them.
    assert_eq!(
        edit(
            &[
                "edit",
                "navigate",
                "--dialect",
                "common-lisp",
                "--name",
                "f",
                "--direction",
                "down",
            ],
            "(defun f (x) x)\n(defun g (y) y)\n"
        ),
        "0.0\n"
    );
}

#[test]
fn yank_reports_an_index_the_ring_does_not_have() {
    let dir = fresh_temp_dir("kill-ring-empty");
    paredit()
        .args(["edit", "yank", "--path", "0", "--index", "3", "--ring"])
        .arg(dir.join("absent.json"))
        .write_stdin("(a)")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--index 3 is out of range"));
}

// --- D42: duplicate, the kill-ring-free half of copy + yank ---

#[test]
fn duplicate_writes_a_second_copy_after_the_form() {
    assert_eq!(
        edit(
            &["edit", "duplicate", "--path", "0.1"],
            "(progn\n  (f y)\n  (g z))\n"
        ),
        "(progn\n  (f y)\n  (f y)\n  (g z))\n"
    );
}

#[test]
fn duplicate_carries_the_comment_block_copy_would_have_taken() {
    assert_eq!(
        edit(
            &["edit", "duplicate", "--path", "0.1"],
            "(progn\n  ;; explain\n  (f y)\n  (g z))\n"
        ),
        "(progn\n  ;; explain\n  (f y)\n  ;; explain\n  (f y)\n  (g z))\n"
    );
}

#[test]
fn duplicate_of_an_inline_form_stays_on_the_line() {
    assert_eq!(
        edit(&["edit", "duplicate", "--path", "0.1"], "(list a b)\n"),
        "(list a a b)\n"
    );
}

#[test]
fn duplicate_leaves_the_kill_ring_alone() {
    let dir = fresh_temp_dir("duplicate-ring");
    let ring = dir.join("ring.json");
    let source = dir.join("source.lisp");
    fs::write(&source, "(progn\n  (f y)\n  (g z))\n").expect("write source");

    // Put something on the ring, then duplicate: the ring entry must survive.
    paredit()
        .args(["edit", "kill", "--path", "0.2", "--to-ring", "--ring"])
        .arg(&ring)
        .arg("--file")
        .arg(&source)
        .arg("--write")
        .assert()
        .success();
    let after_kill = fs::read_to_string(&ring).expect("read ring after kill");

    paredit()
        .args(["edit", "duplicate", "--path", "0.1", "--write", "--file"])
        .arg(&source)
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(&ring).expect("read ring after duplicate"),
        after_kill,
        "duplicate must not push, pop, or reorder the kill ring"
    );
    assert_eq!(
        fs::read_to_string(&source).expect("read source"),
        "(progn\n  (f y)\n  (f y))\n"
    );
}

// --- D39: normalize-quotes, the two spellings of one quote ---

#[test]
fn normalize_quotes_shortens_a_quote_list_to_its_prefix() {
    assert_eq!(
        edit(
            &["edit", "normalize-quotes", "--path", "0.1"],
            "(list (quote x) y)\n"
        ),
        "(list 'x y)\n"
    );
    // `--dialect` is required for the `function` half: stdin with no file name
    // resolves to `Dialect::Unknown`, and `(function f)` is a form only the
    // Common Lisp family has.
    assert_eq!(
        edit(
            &[
                "edit",
                "normalize-quotes",
                "--path",
                "0.1",
                "--dialect",
                "common-lisp"
            ],
            "(mapcar (function car) xs)\n"
        ),
        "(mapcar #'car xs)\n"
    );
}

#[test]
fn normalize_quotes_shortens_every_query_match_with_all() {
    assert_eq!(
        edit(
            &["edit", "normalize-quotes", "--query", "(quote ?x)", "--all"],
            "(list (quote x) (quote y))\n"
        ),
        "(list 'x 'y)\n"
    );
    assert_eq!(
        edit(
            &[
                "edit",
                "normalize-quotes",
                "--query",
                "(function ?x)",
                "--all",
                "--dialect",
                "emacs-lisp"
            ],
            "(list (function car) (function cdr))\n"
        ),
        "(list #'car #'cdr)\n"
    );
}

#[test]
fn normalize_quotes_expands_a_prefix_into_its_list() {
    assert_eq!(
        edit(
            &[
                "edit",
                "normalize-quotes",
                "--path",
                "0.1",
                "--style",
                "longhand"
            ],
            "(list 'x y)\n"
        ),
        "(list (quote x) y)\n"
    );
    assert_eq!(
        edit(
            &[
                "edit",
                "normalize-quotes",
                "--path",
                "0.1",
                "--style",
                "longhand",
                "--dialect",
                "common-lisp"
            ],
            "(mapcar #'car xs)\n"
        ),
        "(mapcar (function car) xs)\n"
    );
}

#[test]
fn normalize_quotes_keeps_a_non_quote_reader_prefix() {
    // The selection's span covers its reader prefix, so shortening the list
    // inside `,(quote x)` used to take the unquote with it and print
    // `(list 'x y)` — an inversion of when the form is evaluated, in output
    // that still reparses and so never tripped the `--write` reparse guard.
    for input in [
        "(list `(quote x) y)",
        "(list ,(quote x) y)",
        "(list ,@(quote x) y)",
        "(list #(quote x) y)",
    ] {
        paredit()
            .args([
                "edit",
                "normalize-quotes",
                "--path",
                "0.1",
                "--dialect",
                "common-lisp",
            ])
            .write_stdin(input.to_owned())
            .assert()
            .failure()
            .stderr(predicate::str::contains("not a quote form"));
    }
}

#[test]
fn normalize_quotes_does_not_write_a_function_form_into_a_clojure_file() {
    // `#'inc` in Clojure is a var quote, and `@x` is a deref; the parser files
    // both under the same catch-all reader-prefix slot as Common Lisp's `#'`.
    // Expanding either into `(function ...)` writes a form Clojure does not
    // have.
    for input in ["(map #'inc xs)", "(map @xs ys)"] {
        paredit()
            .args([
                "edit",
                "normalize-quotes",
                "--path",
                "0.1",
                "--style",
                "longhand",
                "--dialect",
                "clojure",
            ])
            .write_stdin(input.to_owned())
            .assert()
            .failure()
            .stderr(predicate::str::contains("not a quote form"));
    }

    // The universal half still works there.
    assert_eq!(
        edit(
            &[
                "edit",
                "normalize-quotes",
                "--path",
                "0.1",
                "--dialect",
                "clojure"
            ],
            "(map (quote x) xs)\n"
        ),
        "(map 'x xs)\n"
    );
}

#[test]
fn normalize_quotes_leaves_a_form_already_in_that_style_alone() {
    // Running this over a whole file must not depend on knowing which forms
    // already comply.
    assert_eq!(
        edit(
            &["edit", "normalize-quotes", "--path", "0.1"],
            "(list 'x)\n"
        ),
        "(list 'x)\n"
    );
}

#[test]
fn normalize_quotes_refuses_a_form_that_is_not_a_quote() {
    paredit()
        .args(["edit", "normalize-quotes", "--path", "0.1"])
        .write_stdin("(list (f x))")
        .assert()
        .failure()
        .stderr(predicate::str::contains("not a quote form"));
}

// --- K9: reindent ---

#[test]
fn reindent_defun_fixes_indentation_without_rewrapping_lines() {
    assert_eq!(
        edit(
            &["edit", "reindent-defun", "--path", "0"],
            "(defun f (x)\n        (when x\n   (list x)))\n"
        ),
        "(defun f (x)\n  (when x\n    (list x)))\n"
    );
}

#[test]
fn reindent_defun_leaves_a_conventional_definition_byte_identical() {
    let source = "(defun f (x)\n  (when x\n    (list x)))\n";
    assert_eq!(
        edit(&["edit", "reindent-defun", "--path", "0"], source),
        source
    );
}

#[test]
fn reindent_defun_moves_a_top_level_definition_to_column_zero() {
    assert_eq!(
        edit(
            &["edit", "reindent-defun", "--path", "0"],
            "  (defun f (x)\n      (list x))\n"
        ),
        "(defun f (x)\n  (list x))\n"
    );
}

// --- K10: raise --levels ---

#[test]
fn raise_levels_climbs_more_than_one_list_in_one_call() {
    let source = "(when x (let ((y 1)) (f y)))";
    assert_eq!(
        edit(&["edit", "raise", "--path", "0.2.2"], source),
        "(when x (f y))"
    );
    assert_eq!(
        edit(
            &["edit", "raise", "--path", "0.2.2", "--levels", "2"],
            source
        ),
        "(f y)"
    );
}

#[test]
fn raise_levels_names_the_depth_the_selection_actually_had() {
    paredit()
        .args(["edit", "raise", "--path", "0.2", "--levels", "4"])
        .write_stdin("(when x (f y))")
        .assert()
        .failure()
        .stderr(predicate::str::contains("only 1 levels deep"));
}

#[test]
fn raise_levels_below_one_is_a_usage_error() {
    paredit()
        .args(["edit", "raise", "--path", "0.1", "--levels", "0"])
        .write_stdin("(a b)")
        .assert()
        .code(2);
}

// --- K11 / K12: string splitting and escaping ---

#[test]
fn split_string_and_join_are_inverses_through_the_binary() {
    let split = edit(&["edit", "split-string", "--at", "10"], "(list \"foobar\")");
    assert_eq!(split, "(list \"foo\" \"bar\")");
    assert_eq!(
        edit(&["edit", "join", "--path", "0.1"], &split),
        "(list \"foobar\")"
    );
}

#[test]
fn split_string_refuses_an_offset_that_is_not_inside_a_string() {
    paredit()
        .args(["edit", "split-string", "--at", "8"])
        .write_stdin("(list foobar)")
        .assert()
        .failure()
        .stderr(predicate::str::contains("not inside a string literal"));
}

#[test]
fn escape_and_unescape_string_are_inverses() {
    let source = "(list \"a\\\"b\")";
    let escaped = edit(&["edit", "escape-string", "--path", "0.1"], source);
    assert_eq!(escaped, "(list \"a\\\\\\\"b\")");
    assert_eq!(
        edit(&["edit", "unescape-string", "--path", "0.1"], &escaped),
        source
    );
}

#[test]
fn unescape_string_refuses_a_sequence_it_did_not_produce() {
    paredit()
        .args(["edit", "unescape-string", "--path", "0.1"])
        .write_stdin("(list \"a\\nb\")")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unescape only reverses"));
}

// --- K13: context-at ---

#[test]
fn context_at_tells_code_from_string_from_comment() {
    let source = "(foo \"bar\" ; note\n  baz)";
    for (offset, kind) in [
        (1, "code"),
        (7, "string"),
        (14, "comment"),
        (0, "delimiter"),
    ] {
        paredit()
            .args(["inspect", "context-at", "--at", &offset.to_string()])
            .write_stdin(source)
            .assert()
            .success()
            .stdout(predicate::str::contains(format!("kind\t{kind}")));
    }
}

#[test]
fn context_at_reports_the_enclosing_list_and_the_delimiter_stack() {
    let report: serde_json::Value = serde_json::from_str(
        &paredit()
            .args(["inspect", "context-at", "--at", "16", "--output", "json"])
            .write_stdin("(let ((x 1))\n  (+ x 1))")
            .assert()
            .success()
            .get_output()
            .stdout
            .iter()
            .map(|byte| *byte as char)
            .collect::<String>(),
    )
    .expect("context-at emits valid JSON");
    assert_eq!(report["enclosingHead"], "+");
    assert_eq!(report["depth"], 2);
    assert_eq!(report["delimiterStack"], "((");
}

#[test]
fn context_at_gates_on_an_offset_a_character_edit_would_not_survive() {
    paredit()
        .args(["inspect", "context-at", "--at", "7", "--fail-on-structural"])
        .write_stdin("(foo \"bar\")")
        .assert()
        .code(3);
    paredit()
        .args(["inspect", "context-at", "--at", "1", "--fail-on-structural"])
        .write_stdin("(foo \"bar\")")
        .assert()
        .success();
}

// --- K14: transpose between non-adjacent siblings ---

#[test]
fn transpose_swaps_two_siblings_across_a_gap() {
    assert_eq!(
        edit(
            &["edit", "transpose", "--path", "0.1", "--with-path", "0.3"],
            "(alpha beta gamma delta)"
        ),
        "(alpha delta gamma beta)"
    );
}

#[test]
fn transpose_refuses_two_forms_in_different_lists() {
    paredit()
        .args([
            "edit",
            "transpose",
            "--path",
            "0.0.0",
            "--with-path",
            "0.1.0",
        ])
        .write_stdin("((a b) (c d))")
        .assert()
        .failure()
        .stderr(predicate::str::contains("same list"));
}

#[test]
fn transpose_requires_a_second_address() {
    paredit()
        .args(["edit", "transpose", "--path", "0.1"])
        .write_stdin("(a b c)")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "--with-path, --with-at or --with-select",
        ));
}

// --- the whole namespace still writes through the guarded path ---

#[test]
fn the_new_edits_write_in_place_with_reparse_validation() {
    let dir = fresh_temp_dir("paredit-parity-write");
    let file = dir.join("source.lisp");
    fs::write(&file, "(defun f (x) (list x))\n").expect("write fixture");

    paredit()
        .args(["edit", "newline", "--at", "13", "--write", "--file"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::is_empty());

    assert_eq!(
        fs::read_to_string(&file).expect("read rewritten source"),
        "(defun f (x)\n  (list x))\n"
    );
}
