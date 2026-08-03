//! Every rule fires **through the engine**, not by calling `check` directly.
//!
//! Calling `check` bypasses the head index and the dialect filter, which is
//! where a wrong `HeadFilter` or a forgotten `dialect_scope()` shows up. The
//! engine default scope is `COMMON_LISP_ONLY`, so a rule that omitted
//! `dialect_scope()` would pass all 163 unit tests in this package and still
//! never run on a single Racket file.

mod support;

use std::path::Path;

use paredit_core_syntax::dialect::Dialect;

use support::{RULE_NAMES, rules_fired};

fn fired(source: &str) -> Vec<&'static str> {
    rules_fired(source, Dialect::Racket, Path::new("case.rkt"))
}

/// One minimal trigger per rule, driven all the way through the dispatcher.
const TRIGGERS: [(&str, &str); 5] = [
    ("racket-begin0-single-form", "(begin0 (compute))"),
    (
        "racket-case-lambda-single-clause",
        "(case-lambda [(x) (* x 2)])",
    ),
    (
        "racket-for-comprehension-value-discarded",
        "(define (f l) (for/list ([x l]) (displayln x)) 'done)",
    ),
    (
        "racket-match-unreachable-clause",
        "(match x [_ 'other] [(? string?) 'str])",
    ),
    (
        "racket-parameterize-empty-bindings",
        "(parameterize () (run))",
    ),
];

#[test]
fn every_rule_fires_through_the_dispatcher() {
    for (rule, source) in TRIGGERS {
        assert_eq!(
            fired(source),
            vec![rule],
            "{rule} did not reach the dispatcher for: {source}"
        );
    }
}

/// The trigger table covers the published rule list exactly. Without this a
/// rule could be added and silently never driven through the engine at all.
#[test]
fn the_trigger_table_covers_every_published_rule() {
    let mut covered: Vec<&str> = TRIGGERS.iter().map(|(rule, _)| *rule).collect();
    covered.sort_unstable();
    let mut published = RULE_NAMES.to_vec();
    published.sort_unstable();
    assert_eq!(covered, published);
}

/// Not one of these rules runs on Common Lisp, Scheme, Clojure or Emacs Lisp.
/// The dialect filter runs before the walk, so this also proves the package
/// costs a file in another dialect nothing at all.
#[test]
fn no_rule_fires_on_a_dialect_outside_the_scope() {
    // Paren-only so that every reader in the list accepts the bytes; a fixture
    // that failed to parse would prove nothing about scope.
    let sources = [
        "(begin0 (compute))",
        "(case-lambda ((x) (* x 2)))",
        "(define (f l) (for/list ((x l)) (displayln x)) 1)",
        "(match x (_ 1) (2 2))",
        "(parameterize () (run))",
    ];
    for dialect in [
        Dialect::CommonLisp,
        Dialect::Scheme,
        Dialect::Clojure,
        Dialect::EmacsLisp,
    ] {
        for source in sources {
            assert_eq!(
                rules_fired(source, dialect, Path::new("case.txt")),
                Vec::<&str>::new(),
                "{dialect:?} must see none of these rules: {source}"
            );
        }
    }
}

/// The same bytes that fire under Racket fire under nothing else. This is the
/// paired positive for the test above: without it, that one would pass even if
/// every rule had simply stopped working.
#[test]
fn the_racket_scoped_triggers_really_do_fire_as_racket() {
    for (rule, source) in TRIGGERS {
        assert!(
            !fired(source).is_empty(),
            "{rule} fired nothing even as Racket"
        );
    }
}

/// A `#lang` line is the first thing in almost every real Racket file. It must
/// not shift the top-level indices `support::node_context` binary-searches over
/// when it answers the quoting question.
#[test]
fn a_lang_line_does_not_disturb_the_pass() {
    let source = "#lang racket/base\n(require racket/match)\n\
                  (define (f x) (match x [_ 'other] [(? string?) 'str]))\n";
    assert_eq!(fired(source), vec!["racket-match-unreachable-clause"]);
}
