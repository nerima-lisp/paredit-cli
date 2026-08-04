//! Every rule driven through the *real* engine rather than by calling
//! `examine` or `check` directly.
//!
//! Calling `check` bypasses the head index and the dialect filter, which is
//! where the two mistakes this package is most exposed to would hide:
//!
//! - A `HeadFilter::Heads` entry that does not match the source spelling. For
//!   LFE `head_key` returns the head *verbatim*, so `NormalizedHead::new` and
//!   the source have to agree byte for byte — there is no case folding to
//!   rescue a mismatch, and a rule with a wrong head simply never runs.
//!   `match-lambda` is the exposed one: hyphenated, and easy to write as
//!   `match_lambda`.
//! - A forgotten `dialect_scope`. The trait's default is `COMMON_LISP_ONLY`,
//!   so a rule that omits the override silently never fires on LFE while every
//!   unit test on `examine`, which takes the dialect as an argument, still
//!   passes.
//!
//! A sibling batch shipped a rule with a head missing from its `Heads` array
//! and its whole suite stayed green, for exactly this reason.

use std::path::Path;

use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
use paredit_core_lint_engine::policy::RuleSelection;
use paredit_core_lint_engine::rule::RuleCatalog;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;

use crate::ENTRIES;

fn path_for(dialect: Dialect) -> &'static Path {
    match dialect {
        Dialect::Lfe => Path::new("t.lfe"),
        Dialect::Carp => Path::new("t.carp"),
        Dialect::Clojure => Path::new("t.clj"),
        _ => Path::new("t.lisp"),
    }
}

fn outcomes(source: &str, dialect: Dialect) -> Vec<(String, String)> {
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
        let (finding, _fix) = outcome.into_parts();
        (finding.rule.to_owned(), finding.message.clone())
    })
    .collect()
}

/// The rule names that fire on `source`, sorted so the assertions do not
/// depend on registration order.
pub(crate) fn fired(source: &str, dialect: Dialect) -> Vec<String> {
    let mut names: Vec<String> = outcomes(source, dialect)
        .into_iter()
        .map(|(rule, _)| rule)
        .collect();
    names.sort();
    names
}

// -- both rules reach the engine -----------------------------------------

#[test]
fn every_rule_fires_through_the_real_dispatch() {
    assert_eq!(
        fired(
            "(defun f ((x) (when (lists:member x '(1 2))) 'yes) ((_) 'no))",
            Dialect::Lfe
        ),
        vec!["lfe-illegal-guard-call"]
    );
    assert_eq!(
        fired(
            "(defun a (x) (case x ('one 1) (_ 'fallback) ('two 2)))",
            Dialect::Lfe
        ),
        vec!["lfe-clause-after-catch-all"]
    );
}

/// Each head has to be indexed independently. Deleting any one of the four
/// from `HEADS` must fail a test here; deleting one and finding the suite
/// still green is how a rule ships with a head it never sees.
#[test]
fn each_clause_head_is_indexed_separately() {
    // case
    assert_eq!(
        fired("(defun a (x) (case x (_ 'any) ('two 2)))", Dialect::Lfe).len(),
        1
    );
    // receive
    assert_eq!(
        fired("(defun a () (receive (_ 'any) ('two 2)))", Dialect::Lfe).len(),
        1
    );
    // match-lambda -- hyphenated, and the one most likely to be misspelled
    assert_eq!(
        fired("(match-lambda ((_) 'any) (('two) 2))", Dialect::Lfe).len(),
        1
    );
    // defun, in its matching form
    assert_eq!(
        fired("(defun a ((_) 'any) (('two) 2))", Dialect::Lfe).len(),
        1
    );
}

/// The guard rule anchors on `when` alone. If that head were wrong the rule
/// would never run at all, and every `examine`-level test would still pass.
#[test]
fn the_guard_head_is_indexed() {
    assert_eq!(fired("(when (lists:member x y))", Dialect::Lfe).len(), 1);
}

/// Both rules in one document, each firing once.
#[test]
fn both_rules_fire_in_one_document() {
    let source = "\
(defun a (x)
  (case x
    ('one 1)
    (_ 'fallback)
    ('two 2)))
(defun b
  ((x) (when (lists:member x '(1 2))) 'yes)
  ((_) 'no))
";
    let mut names = fired(source, Dialect::Lfe);
    names.sort();
    assert_eq!(
        names,
        vec!["lfe-clause-after-catch-all", "lfe-illegal-guard-call"]
    );
}

// -- the dialect gate, through dispatch ----------------------------------

/// `case` and `when` are live operators in Common Lisp, Emacs Lisp, Clojure
/// and Scheme with entirely different meanings — `(when test body)` is an
/// ordinary conditional, not a guard — so firing there would be a real false
/// positive, not merely noise.
#[test]
fn the_dialect_scope_holds_through_dispatch() {
    for dialect in [
        Dialect::CommonLisp,
        Dialect::Clojure,
        Dialect::Scheme,
        Dialect::EmacsLisp,
        Dialect::Fennel,
        Dialect::Janet,
        Dialect::Carp,
    ] {
        assert!(
            fired("(defun a (x) (case x ('one 1) (_ 'f) ('two 2)))", dialect).is_empty(),
            "{dialect:?} must be out of scope for the clause rule"
        );
        assert!(
            fired("(when (lists:member x y))", dialect).is_empty(),
            "{dialect:?} must be out of scope for the guard rule"
        );
    }
}

/// The single most important negative: an ordinary Common Lisp `when` is a
/// conditional whose body is arbitrary code, and `lists:member` is not even
/// valid there — but a rule that forgot `dialect_scope` would still be given
/// this node, because `when` is in its `Heads`.
#[test]
fn a_common_lisp_conditional_is_never_a_guard() {
    assert!(fired("(when (foo:bar x) (do-something))", Dialect::CommonLisp).is_empty());
}

// -- correct code stays silent -------------------------------------------

#[test]
fn correct_lfe_produces_nothing() {
    let source = "\
(defmodule m
  (export (classify 1) (loop 0)))

(defun classify
  ((x) (when (is_integer x)) 'int)
  ((x) (when (erlang:is_atom x)) 'atom)
  ((_) 'other))

(defun loop ()
  (receive
    ('ping 'pong)
    (msg (when (is_tuple msg)) 'tuple)
    (after 1000 'timeout)))

(defun add (a b) (+ a b))
";
    assert!(fired(source, Dialect::Lfe).is_empty());
}

// -- quoting, through dispatch -------------------------------------------

/// A hard-quoted form is data. The dispatcher hands quoted nodes to rules like
/// any other, so this is the rule's own responsibility.
#[test]
fn a_hard_quoted_form_is_not_reported() {
    assert!(
        fired(
            "(defun f () '(case x ('one 1) (_ 'any) ('two 2)))",
            Dialect::Lfe
        )
        .is_empty()
    );
    assert!(fired("(defun f () '(when (lists:member x y)))", Dialect::Lfe).is_empty());
}

/// A macro template is suppressed too, and that is a deliberate limitation
/// rather than an accident — pinned here so it stays visible.
///
/// A dead clause written into a `` ` `` template really does become a dead
/// clause at every expansion site, so there are true positives being given up.
/// The reason to give them up is that a template's clause list is usually not
/// all there: `` `(case ,x ,@clauses) `` splices clauses this rule cannot see,
/// so a catch-all that looks last may not be, and one that looks first may be
/// preceded by spliced clauses. Reporting on templates would therefore invent
/// findings as often as it found them.
///
/// Suppressing costs recall; reporting would cost precision on the shape LFE
/// uses most. This is the side to be wrong on.
#[test]
fn a_quasiquoted_template_is_suppressed_by_the_conservative_quote_model() {
    assert!(
        fired(
            "(defmacro m (x) `(case ,x ('one 1) (_ 'any) ('two 2)))",
            Dialect::Lfe
        )
        .is_empty()
    );
    assert!(
        fired(
            "(defmacro m (x) `(when (lists:member ,x '(1 2))))",
            Dialect::Lfe
        )
        .is_empty()
    );
}

// -- syntax-rules templates ----------------------------------------------

/// A `defsyntax` rule is a pattern/template pair written with no quoting at
/// all, so its symbols are *pattern variables* rather than the bare variables
/// they look like.
///
/// This is taken verbatim from LFE's own `dev/test_macro.lfe:27`, where it was
/// the corpus audit's only third-party clause finding. `(p . b)` reads as a
/// bare-variable catch-all, making `(_ (c-ond . c))` look dead; `p` is in fact
/// replaced by whatever pattern the caller wrote.
#[test]
fn a_defsyntax_template_is_not_code() {
    let source = "\
(defsyntax c-ond
  ([('else . b)] (begin . b))
  ([(('?= p e) . b) . c] (case e (p . b) (_ (c-ond . c))))
  ([] 'false))
";
    assert!(fired(source, Dialect::Lfe).is_empty());
}

/// The whole `defsyntax` family, including the `scm:` spelling the guide uses.
#[test]
fn every_syntax_template_form_suppresses() {
    for head in [
        "defsyntax",
        "scm:defsyntax",
        "define-syntax",
        "let-syntax",
        "syntaxlet",
        "syntax-rules",
    ] {
        let source =
            format!("({head} m (case e (p 1) (_ 'any) ('two 2)) (when (lists:member x y)))");
        assert!(
            fired(&source, Dialect::Lfe).is_empty(),
            "{head} must suppress findings in its template"
        );
    }
}

/// But the suppression must be specific. An ordinary `defun` named something
/// similar is still code, or the gate would silence the rule wholesale.
#[test]
fn a_form_that_merely_looks_like_a_template_head_is_still_code() {
    assert_eq!(
        fired(
            "(defun defsyntax-helper (x) (case x ('one 1) (_ 'any) ('two 2)))",
            Dialect::Lfe
        ),
        vec!["lfe-clause-after-catch-all"]
    );
    assert_eq!(
        fired(
            "(defun my-syntax (x) (case x ('one 1) (_ 'any) ('two 2)))",
            Dialect::Lfe
        ),
        vec!["lfe-clause-after-catch-all"]
    );
}

// -- the message ----------------------------------------------------------

#[test]
fn the_messages_name_the_defect() {
    let found = outcomes(
        "(defun a (x) (case x ('one 1) (_ 'fallback) ('two 2)))",
        Dialect::Lfe,
    );
    assert_eq!(found.len(), 1);
    assert!(
        found[0].1.contains("can never run"),
        "message was {:?}",
        found[0].1
    );
    assert!(found[0].1.contains("case"), "message was {:?}", found[0].1);

    let found = outcomes("(when (lists:member x '(1 2)))", Dialect::Lfe);
    assert_eq!(found.len(), 1);
    assert!(
        found[0].1.contains("illegal guard expression"),
        "message was {:?}",
        found[0].1
    );
    assert!(found[0].1.contains("lists"), "message was {:?}", found[0].1);
}

/// `=:=` contains a colon and appears in almost every real LFE guard. A
/// regression here would fire on most of the corpus, so it is pinned through
/// dispatch as well as in the domain.
#[test]
fn a_comparison_operator_never_fires_through_dispatch() {
    assert!(fired("(when (=:= x 'one))", Dialect::Lfe).is_empty());
    assert!(
        fired(
            "(defun f ((x) (when (=:= x 'one)) 'yes) ((_) 'no))",
            Dialect::Lfe
        )
        .is_empty()
    );
}
