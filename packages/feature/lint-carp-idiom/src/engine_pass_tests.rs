//! Every rule driven through the *real* engine rather than by calling
//! `examine` or `check` directly.
//!
//! Calling `check` bypasses the head index and the dialect filter, which is
//! where the two mistakes this package is most exposed to would hide:
//!
//! - A `HeadFilter::Heads` entry that does not match the source spelling. For
//!   Carp `head_key` returns the head *verbatim*, so `NormalizedHead::new` and
//!   the source have to agree byte for byte — there is no case folding to
//!   rescue a mismatch, and a rule with a wrong head simply never runs. `=>`
//!   and `==>` are pure punctuation, so this is the whole safety net.
//! - A forgotten `dialect_scope`. The trait's default is `COMMON_LISP_ONLY`,
//!   so a rule that omits the override silently never fires on Carp while
//!   every unit test on `examine`, which takes the dialect as an argument,
//!   still passes.

use std::path::Path;

use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
use paredit_core_lint_engine::policy::RuleSelection;
use paredit_core_lint_engine::rule::RuleCatalog;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;

use crate::ENTRIES;

fn path_for(dialect: Dialect) -> &'static Path {
    match dialect {
        Dialect::Carp => Path::new("t.carp"),
        Dialect::Clojure => Path::new("t.clj"),
        _ => Path::new("t.lisp"),
    }
}

fn outcomes(source: &str, dialect: Dialect) -> Vec<(String, String, bool)> {
    let catalog = RuleCatalog::new(&ENTRIES);
    let index = build_head_index(catalog);
    let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
    collect_lint_outcomes(
        catalog,
        &index,
        path_for(dialect),
        dialect,
        &tree,
        source,
        RuleSelection::All,
    )
    .expect("lint pass")
    .into_iter()
    .map(|outcome| {
        let (finding, fix) = outcome.into_parts();
        (
            finding.rule.to_owned(),
            finding.message.clone(),
            fix.is_some(),
        )
    })
    .collect()
}

/// `source` with every offered fix applied, so a test can assert on the text
/// the user would actually end up with rather than merely that a fix exists.
///
/// Applies right to left so an earlier edit cannot shift a later span.
fn fixed_source(source: &str, dialect: Dialect) -> String {
    let catalog = RuleCatalog::new(&ENTRIES);
    let index = build_head_index(catalog);
    let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
    let mut edits: Vec<(usize, usize, String)> = collect_lint_outcomes(
        catalog,
        &index,
        path_for(dialect),
        dialect,
        &tree,
        source,
        RuleSelection::All,
    )
    .expect("lint pass")
    .into_iter()
    .filter_map(|outcome| outcome.into_parts().1)
    .flat_map(|fix| {
        fix.replacements()
            .map(|replacement| {
                (
                    replacement.span().start().get(),
                    replacement.span().end().get(),
                    replacement.text().to_owned(),
                )
            })
            .collect::<Vec<_>>()
    })
    .collect();
    edits.sort_by_key(|(start, _, _)| std::cmp::Reverse(*start));

    let mut out = source.to_owned();
    for (start, end, text) in edits {
        out.replace_range(start..end, &text);
    }
    out
}

/// The whole point of a `Fixability::Fixable` rule: the rewrite has to produce
/// the right bytes. Asserting only that a fix *exists* would pass just as well
/// for a fix that replaced the wrong span.
#[test]
fn the_fix_rewrites_only_the_operator() {
    assert_eq!(
        fixed_source("(=> state (update-pos) (draw))", Dialect::Carp),
        "(-> state (update-pos) (draw))"
    );
    assert_eq!(
        fixed_source("(==> results (Array.copy-filter &f))", Dialect::Carp),
        "(--> results (Array.copy-filter &f))"
    );
}

/// Several fixes in one document must not corrupt each other's spans, and the
/// `@(…)` shape the reader mis-lexes must survive the rewrite untouched.
#[test]
fn several_fixes_in_one_document_compose() {
    assert_eq!(
        fixed_source("(=> a @(f b))\n(==> c &(g d))\n(-> e h)", Dialect::Carp),
        "(-> a @(f b))\n(--> c &(g d))\n(-> e h)"
    );
}

/// A fix that is offered must leave the file parseable, and re-linting the
/// result must find nothing left to do.
#[test]
fn the_fixed_source_is_clean_and_stable() {
    let once = fixed_source("(=> a inc)\n(==> b dec)", Dialect::Carp);
    SyntaxTree::parse_with_dialect(&once, Dialect::Carp).expect("the fixed source must parse");
    assert!(
        fired(&once, Dialect::Carp).is_empty(),
        "the rewrite must remove the finding, not move it"
    );
    assert_eq!(
        fixed_source(&once, Dialect::Carp),
        once,
        "and be idempotent"
    );
}

/// The rule names that fire on `source`, sorted so the assertions do not
/// depend on registration order.
pub(crate) fn fired(source: &str, dialect: Dialect) -> Vec<String> {
    let mut names: Vec<String> = outcomes(source, dialect)
        .into_iter()
        .map(|(rule, _, _)| rule)
        .collect();
    names.sort();
    names
}

// -- the rule reaches the engine -----------------------------------------

#[test]
fn every_rule_fires_through_the_real_dispatch() {
    assert_eq!(
        fired("(defn f [x] (=> x inc inc))", Dialect::Carp),
        vec!["carp-deprecated-thread-macro"]
    );
    assert_eq!(
        fired("(defn f [x] (==> x inc inc))", Dialect::Carp),
        vec!["carp-deprecated-thread-macro"]
    );
}

/// Both heads have to be in `HEADS` independently. Deleting either one from
/// the array leaves the other's test green, which is exactly how a sibling
/// batch shipped a rule with a missing head.
#[test]
fn each_head_is_indexed_separately() {
    assert_eq!(fired("(=> a b)", Dialect::Carp).len(), 1);
    assert_eq!(fired("(==> a b)", Dialect::Carp).len(), 1);
    // Both in one document: two findings, not one.
    assert_eq!(fired("(=> a b)\n(==> c d)", Dialect::Carp).len(), 2);
}

/// The dialect gate, through the engine. `=>` is a live operator in Clojure
/// test code, so firing there would be a real false positive.
#[test]
fn the_dialect_scope_holds_through_dispatch() {
    assert!(fired("(=> a b)", Dialect::Clojure).is_empty());
    assert!(fired("(=> a b)", Dialect::CommonLisp).is_empty());
    assert!(fired("(=> a b)", Dialect::Fennel).is_empty());
    assert!(fired("(=> a b)", Dialect::Janet).is_empty());
}

#[test]
fn the_supported_spelling_never_fires() {
    assert!(fired("(-> a b)\n(--> c d)", Dialect::Carp).is_empty());
}

// -- the fix -------------------------------------------------------------

#[test]
fn a_plain_use_carries_a_fix() {
    let found = outcomes("(=> a inc)", Dialect::Carp);
    assert_eq!(found.len(), 1);
    assert!(found[0].2, "expected a fix to be offered");
}

/// The shadowing guard, through the engine: a file that defines its own `->`
/// still gets the finding, but not the rewrite.
#[test]
fn a_shadowed_replacement_reports_without_a_fix() {
    let source = "(defmacro -> [:rest forms] (my-own-threading forms))\n(=> a inc)";
    let found = outcomes(source, Dialect::Carp);
    assert_eq!(found.len(), 1, "the finding must still be reported");
    assert!(
        !found[0].2,
        "the fix must be withheld when `->` is shadowed"
    );
}

/// Carp puts most definitions inside a `defmodule`, so the shadowing scan has
/// to descend into container bodies. A scan that only looked at top level
/// would miss this and offer a rewrite to a `->` that is not core's.
#[test]
fn a_replacement_shadowed_inside_a_module_still_withholds_the_fix() {
    let source = "(defmodule M\n  (defmacro -> [:rest forms] (mine forms))\n)\n(=> a inc)";
    let found = outcomes(source, Dialect::Carp);
    assert_eq!(found.len(), 1, "the finding must still be reported");
    assert!(
        !found[0].2,
        "a `->` defined inside a defmodule shadows core's just as well"
    );
}

/// Shadowing the *other* replacement must not suppress this one's fix.
#[test]
fn shadowing_is_matched_against_the_right_replacement() {
    let source = "(defmacro --> [:rest forms] (mine forms))\n(=> a inc)";
    let found = outcomes(source, Dialect::Carp);
    assert_eq!(found.len(), 1);
    assert!(
        found[0].2,
        "`-->` being shadowed says nothing about rewriting `=>` to `->`"
    );
}

// -- quoting -------------------------------------------------------------

/// A deprecated head inside quoted data is not a call to it.
#[test]
fn a_quoted_use_is_not_reported() {
    // The `'` reader prefix.
    assert!(fired("(def forms '(=> a b))", Dialect::Carp).is_empty());
    // And the long-hand spellings, which hand-written Carp macros use and
    // which `docs/Quasiquotation.md` presents as the primary form.
    assert!(fired("(quote (=> a b))", Dialect::Carp).is_empty());
    assert!(fired("(quasiquote (=> a b))", Dialect::Carp).is_empty());
}

/// But a macro that genuinely emits the deprecated spelling into a template is
/// still a use, and the conservative quote model means this one is suppressed.
/// Pinned so the limitation is visible rather than assumed away.
#[test]
fn a_quasiquoted_use_is_suppressed_by_the_conservative_quote_model() {
    assert!(
        fired("(defmacro m [x] `(=> %x inc))", Dialect::Carp).is_empty(),
        "Carp's `%` unquote is not a reader prefix here, so the whole template reads as data"
    );
}

// -- the reader's arity inflation ----------------------------------------

/// `@(…)` and `&(…)` give the enclosing call an extra child. The rule keys on
/// the head alone precisely so this cannot hide a finding.
#[test]
fn a_call_whose_arity_the_reader_inflates_still_fires() {
    assert_eq!(fired("(=> x @(f y))", Dialect::Carp).len(), 1);
    assert_eq!(fired("(==> x &(g y))", Dialect::Carp).len(), 1);
    assert_eq!(fired("(=> @(a b) &(c d) @e)", Dialect::Carp).len(), 1);
}
