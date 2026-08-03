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
//! The correct corpus deliberately includes `@(…)` and `&(…)` forms, which
//! this workspace's reader mis-lexes into an extra sibling atom (see
//! `crate::support`). They belong here precisely because the rule must be
//! immune to that: it keys on the head symbol and never on arity.

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;

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

/// The correct corpus exercises the reader's `@(…)` / `&(…)` arity inflation.
/// Pinned so a later edit that "simplifies" the corpus does not quietly remove
/// the only coverage of the shape the rule had to be built around.
#[test]
fn the_correct_corpus_exercises_the_readers_arity_inflation() {
    let tree = SyntaxTree::parse_with_dialect(CARP_CORPUS, Dialect::Carp).expect("parse");
    fn count_bare(view: &paredit_core_syntax::sexpr::ExpressionView, n: &mut usize) {
        for child in &view.children {
            if matches!(child.text.as_deref(), Some("@") | Some("&")) {
                *n += 1;
            }
            count_bare(child, n);
        }
    }
    let mut bare = 0;
    count_bare(&tree.root_view(), &mut bare);
    assert!(
        bare > 0,
        "the corpus should contain `@(…)`/`&(…)`, which the reader splits into a bare sigil atom"
    );
}
