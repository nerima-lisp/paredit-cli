//! Pins that `defrule`'s pattern grammar and the `--query`/`--rewrite`
//! engine's grammar do not drift apart.
//!
//! Before this contract existed, `defrule` had its own ~230-line pattern
//! matcher (`packages/feature/lint-custom/src/pattern.rs`, pre-unification)
//! with no test anywhere comparing it against `packages/core/syntax/src/
//! selector`'s — the two silently disagreed on `...` mid-list, on reader
//! prefixes, and on whether a literal string compared case-exactly. `defrule`
//! is a thin front end over the selector's own
//! [`classify_atom`], so this file has two layers:
//!
//! - a structural check that the front end still *calls* the shared
//!   tokenizer rather than re-growing its own copy of it, and
//! - behavioral checks that a representative pattern using each new
//!   capability (`?name:kind`, mid-list `?name...`, a reader-prefix
//!   constraint) matches the identical set of forms under `inspect lint
//!   --custom-rules` and under `query find --query`.
//!
//! The two spellings `defrule` deliberately keeps at their pre-unification
//! meaning (`_` and `?_`; see `packages/feature/lint-custom/src/pattern.rs`'s
//! module documentation) are exactly what these tests do *not* compare,
//! since the two engines are documented to disagree there on purpose.

use super::*;

/// Writes one `defrule` naming `pattern`, and returns the rule directory.
fn rule_dir_for(name: &str, pattern: &str) -> PathBuf {
    let dir = fresh_temp_dir(name).join("rules");
    fs::create_dir_all(&dir).expect("create rule dir");
    fs::write(
        dir.join("house.lisp"),
        format!(r#"(defrule sync-check :pattern {pattern} :message "m")"#),
    )
    .expect("write house.lisp");
    dir
}

/// How many `sync-check` findings `inspect lint --custom-rules` reports over
/// `source`.
///
/// Filtered by rule name, not `finding_count`, so a shipped rule that happens
/// to also fire over the fixture cannot make this test flaky or, worse, pass
/// for the wrong reason.
fn defrule_match_count(pattern: &str, source: &str) -> usize {
    let rules = rule_dir_for("pattern-grammar-sync-defrule", pattern);
    let dir = fresh_temp_dir("pattern-grammar-sync-defrule-src");
    let file = dir.join("a.lisp");
    fs::write(&file, source).expect("write a.lisp");

    let output = paredit()
        .args(["inspect", "lint", "--custom-rules"])
        .arg(&rules)
        .args(["--output", "json"])
        .arg(&file)
        .output()
        .expect("run inspect lint");
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("inspect lint emits json");
    value["findings"]
        .as_array()
        .expect("findings array")
        .iter()
        .filter(|finding| finding["rule"] == "sync-check")
        .count()
}

/// How many matches `query find --query` reports over `source`.
fn query_match_count(pattern: &str, source: &str) -> usize {
    let dir = fresh_temp_dir("pattern-grammar-sync-query-src");
    let file = dir.join("a.lisp");
    fs::write(&file, source).expect("write a.lisp");

    let output = paredit()
        .args(["query", "find", "--query", pattern])
        .arg(&file)
        .output()
        .expect("run query find");
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("query find emits json");
    value["finding_count"].as_u64().expect("finding_count") as usize
}

/// Reusable across every case below: both engines must report the same
/// number of matches for the same pattern and source.
fn assert_engines_agree(pattern: &str, matching_source: &str, non_matching_source: &str) {
    assert_eq!(
        defrule_match_count(pattern, matching_source),
        query_match_count(pattern, matching_source),
        "defrule and query disagree over {pattern:?} against a form meant to match"
    );
    assert_eq!(
        defrule_match_count(pattern, non_matching_source),
        query_match_count(pattern, non_matching_source),
        "defrule and query disagree over {pattern:?} against a form meant not to match"
    );
}

#[test]
fn defrule_and_query_agree_on_a_typed_capture() {
    assert_engines_agree(
        "(f ?a:number)",
        "(defun run () (f 1))\n",
        "(defun run () (f x))\n",
    );
}

#[test]
fn defrule_and_query_agree_on_a_named_mid_list_rest() {
    // Only reachable in `defrule` through the new, explicit `?name...`
    // spelling: the pre-unification grammar gave a mid-list `...` no rest
    // meaning at all (see the module documentation on the non-trailing-`...`
    // compatibility case, which this deliberately does not exercise). `(x)` is
    // one form short of the two fixed positions `?a`/`?b` require, whatever
    // `?mid...` swallows in between — genuinely unmatched by both engines,
    // not merely matched with an empty middle the way `(x y)` would be.
    assert_engines_agree("(?a ?mid... ?b)", "(x 1 2 3 y)\n", "(x)\n");
}

#[test]
fn defrule_and_query_agree_on_a_reader_prefix_constraint() {
    assert_engines_agree("(mapcar #'?fn ?xs)", "(mapcar #'f xs)\n", "(mapcar f xs)\n");
}

#[test]
fn defrule_and_query_agree_that_a_string_literal_compares_exactly() {
    assert_engines_agree(r#"(f "a")"#, "(f \"a\")\n", "(f \"A\")\n");
}

/// The structural half: `defrule`'s pattern reader must still *call* the
/// selector's own tokenizer for every spelling it does not special-case,
/// rather than re-growing an independent copy of the grammar it decides.
///
/// A behavioral test alone would not catch a change that kept today's
/// spellings working by accident while quietly forking the grammar for a
/// spelling nobody wrote a case for yet; this is what pins the *mechanism*,
/// not just today's examples of it.
#[test]
fn defrule_pattern_reading_delegates_to_the_selectors_tokenizer() {
    let source = fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/packages/feature/lint-custom/src/pattern.rs"
    ))
    .expect("read lint-custom's pattern.rs");
    assert!(
        source.contains("classify_atom("),
        "defrule's pattern reader no longer calls the selector's shared \
         classify_atom tokenizer; the two pattern grammars can now drift \
         apart silently"
    );
}
