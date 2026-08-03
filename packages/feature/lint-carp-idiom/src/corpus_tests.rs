//! A permanent corpus: realistic *correct* Carp that must stay silent, and a
//! dangerous twin that must fire every rule exactly once.
//!
//! The silent half is worthless on its own. A rule whose head is misspelled,
//! whose dialect scope is wrong, or that was deleted entirely also produces
//! zero findings here — so the corpus asserts a **candidate count** as well,
//! taken from the same `candidate_count` the audit used. A zero-finding sweep
//! over zero candidates says nothing; a zero-finding sweep over five threading
//! macro calls says the rule looked and declined.
//!
//! The idioms are taken from real code: the shapes below follow
//! `carp-lang/Carp`'s own `core/` and `examples/` — `defmodule` wrapping
//! `defn`, a `sig` beside its definition, `&` on borrowed arguments, `@` to
//! take an owned copy, `Array.reduce` with a `&(fn …)`, `let-do` for
//! sequencing, and `-> `/`-->` for threading.
//!
//! The correct corpus deliberately includes `@(…)` and `&(…)` forms. The
//! reader used to mis-lex them into an extra sibling atom; it now reads them as
//! reader prefixes (see `crate::support`). They stay here either way, because
//! the rule must be immune to how they lex: it keys on the head symbol and
//! never on arity.

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ExpressionKind, ExpressionView, ReaderPrefix, SyntaxTree};

use crate::deprecated_thread_macro;
use crate::engine_pass_tests::fired;

/// Correct, idiomatic Carp. Every rule in this package must decline it.
const CARP_CORPUS: &str = r#";; A small module in the style of Carp's own core/.
(defmodule Stats

  (doc mean "the arithmetic mean of `xs`.")
  (sig mean (Fn [&(Array Double)] Double))
  (defn mean [xs]
    (let [total (Array.reduce &(fn [acc x] (+ acc @x)) 0.0 xs)
          n (Array.length xs)]
      (if (= n 0)
        0.0
        (/ total (from-int n)))))

  (doc normalize "scales `xs` into the unit range.")
  (defn normalize [xs]
    (let [top (Array.reduce &(fn [acc x] (Double.max acc @x)) 0.0 xs)]
      (if (= top 0.0)
        @xs
        (Array.copy-map &(fn [x] (/ @x top)) xs))))

  (doc summary "a human-readable one-line summary of `xs`.")
  (defn summary [xs]
    (-> (mean xs)
        (Double.to-string)
        (String.append " avg")))

  (doc describe "the same, threaded the other way.")
  (defn describe [xs]
    (--> (mean xs)
         (Double.to-string)
         (String.append "average: ")))
)

(deftype Point [x Double y Double])

(defmodule Point
  (defn shifted [p dx]
    (Point.set-x @p (+ @(Point.x p) dx)))

  (defn label [p]
    (-> (Point.x p)
        (Double.copy)
        (Double.to-string)))

  (defn tagged [p]
    (--> (Point.y p)
         (Double.copy)
         (Double.to-string)
         (String.append "y=")))
)

(defn main []
  (let-do [points [(Point.init 1.0 2.0) (Point.init 3.0 4.0)]
           names (Array.copy-map &Point.label &points)]
    (println* &(String.join ", " &names))
    (println* &(Stats.summary &[1.0 2.0 3.0]))))
"#;

/// The same code with each rule's defect introduced exactly once.
const CARP_DANGEROUS: &str = r#"(defmodule Stats
  (defn summary [xs]
    (=> (mean xs)
        (Double.to-string)
        (String.append " avg")))

  (defn describe [xs]
    (==> (mean xs)
         (Double.to-string)
         (String.append "average: ")))
)
"#;

#[test]
fn the_correct_corpus_parses() {
    SyntaxTree::parse_with_dialect(CARP_CORPUS, Dialect::Carp)
        .expect("the correct corpus must parse");
    SyntaxTree::parse_with_dialect(CARP_DANGEROUS, Dialect::Carp)
        .expect("the dangerous corpus must parse");
}

#[test]
fn correct_carp_yields_no_findings() {
    assert_eq!(
        fired(CARP_CORPUS, Dialect::Carp),
        Vec::<String>::new(),
        "idiomatic Carp must be silent"
    );
}

/// The denominator, without which the assertion above is a false-clean.
#[test]
fn the_correct_corpus_actually_contains_candidates() {
    let tree = SyntaxTree::parse_with_dialect(CARP_CORPUS, Dialect::Carp).expect("parse");
    let candidates = deprecated_thread_macro::domain::candidate_count(Dialect::Carp, &tree);
    assert!(
        candidates >= 4,
        "the corpus must exercise the rule; got {candidates} threading macro calls"
    );
    // And none of them is a deprecated spelling.
    assert!(
        deprecated_thread_macro::domain::collect(Dialect::Carp, &tree).is_empty(),
        "the correct corpus must use only the supported spellings"
    );
}

#[test]
fn the_dangerous_twin_fires_each_rule_exactly_once() {
    let found = fired(CARP_DANGEROUS, Dialect::Carp);
    assert_eq!(
        found,
        vec![
            "carp-deprecated-thread-macro".to_owned(),
            "carp-deprecated-thread-macro".to_owned(),
        ],
        "each deprecated spelling must be reported once"
    );
}

/// The correct corpus exercises Carp's `@(…)` / `&(…)` reader prefixes.
///
/// This pin used to run the other way. It asserted that the reader split
/// `@(…)` into a bare `@` atom plus a sibling list, inflating the enclosing
/// call's arity by one, because that is what the reader then did: Carp shared
/// the permissive legacy reader, which implements neither sigil. `core/syntax`
/// now implements Carp's own `sexpr` dispatch, so each sigil is a
/// [`ReaderPrefix`] on the form it prefixes and no bare sigil atom survives.
///
/// Both halves stay pinned, and they fail in opposite directions:
///
/// * The zero-bare-sigil half fails if the reader regresses to the old split,
///   which is the shape every rule in this package must stay immune to.
/// * The prefixed-form count fails if a later edit "simplifies" the corpus and
///   quietly removes the only coverage of that shape. The expected 8 is read
///   off `CARP_CORPUS` itself, not off the parser: seven `&(`/`@(` paren forms
///   (the `sig` argument type, three `&(fn …)` arguments, `@(Point.x p)`, and
///   the two `println*` arguments) plus the one `&[1.0 2.0 3.0]` array.
#[test]
fn the_correct_corpus_exercises_carps_sigil_reader_prefixes() {
    let tree = SyntaxTree::parse_with_dialect(CARP_CORPUS, Dialect::Carp).expect("parse");

    fn walk(view: &ExpressionView, bare: &mut usize, prefixed_lists: &mut usize) {
        for child in &view.children {
            if matches!(child.text.as_deref(), Some("@") | Some("&")) {
                *bare += 1;
            }
            if child.kind == ExpressionKind::List
                && child
                    .reader_prefixes
                    .iter()
                    .any(|prefix| matches!(prefix, ReaderPrefix::Copy | ReaderPrefix::Ref))
            {
                *prefixed_lists += 1;
            }
            walk(child, bare, prefixed_lists);
        }
    }

    let mut bare = 0;
    let mut prefixed_lists = 0;
    walk(&tree.root_view(), &mut bare, &mut prefixed_lists);

    assert_eq!(
        bare, 0,
        "no bare `@`/`&` sigil atom may survive the Carp reader"
    );
    assert_eq!(
        prefixed_lists, 8,
        "the corpus must keep its `@(…)`/`&(…)` coverage"
    );
}
