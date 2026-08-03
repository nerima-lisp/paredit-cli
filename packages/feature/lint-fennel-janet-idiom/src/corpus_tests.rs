//! A permanent corpus per dialect: realistic *correct* code that must stay
//! silent, and a dangerous twin that must fire every rule exactly once.
//!
//! The silent half is worthless on its own. A rule whose head is misspelled,
//! whose dialect scope is wrong, or that was deleted entirely also produces
//! zero findings here — so each corpus asserts a **candidate count** as well,
//! taken from the same `candidate_count` the audit used. A zero-finding sweep
//! over zero candidates says nothing; a zero-finding sweep over sixteen
//! candidates says the rules looked and declined.
//!
//! The idioms are taken from real code: the Fennel corpus follows the shapes in
//! `fennel-lang/fennel`'s own `test/` and `reference.md`, and the Janet corpus
//! follows `janet-lang/spork` and `jpm` — accumulate into a `buffer`, group into
//! an `@{}`, `+=` a counter.

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;

use crate::engine_pass_tests::fired;
use crate::{
    fennel_deprecated_form, fennel_each_over_non_iterator, janet_empty_loop_body,
    janet_mutating_immutable_literal, var_never_set,
};

/// Correct, idiomatic Fennel. Every rule in this package must decline it.
const FENNEL_CORPUS: &str = r#";; A small module in the style of the reference's own examples.
(local {: view} (require :fennel.view))
(import-macros {: when-some} :my.macros)

(fn count-keys [tbl]
  "How many keys tbl has."
  (var total 0)
  (each [_ _ (pairs tbl)]
    (set total (+ total 1)))
  total)

(fn render-all [items]
  (local out [])
  (each [_ item (ipairs items)]
    (table.insert out (view item)))
  (table.concat out "\n"))

(fn tally [xs]
  (var running 0)
  (for [i 1 (length xs)]
    (set running (+ running (. xs i))))
  running)

(fn first-match [xs pred]
  (var found nil)
  (each [_ x (ipairs xs) &until found]
    (when (pred x)
      (set found x)))
  found)

(fn lines-of [handle]
  (local acc [])
  (each [line (handle:lines)]
    (table.insert acc line))
  acc)

(macro incr [place]
  `(set ,place (+ ,place 1)))

{: count-keys : render-all : tally : first-match : lines-of}
"#;

/// Correct, idiomatic Janet.
const JANET_CORPUS: &str = r#"# A small module in the style of spork and jpm.
(import spork/path)

(defn tally
  "Sum every value of ds."
  [ds]
  (var total 0)
  (each x ds
    (+= total x))
  total)

(defn group-by
  "Bucket ds by (f x)."
  [f ds]
  (def groups @{})
  (loop [x :in ds]
    (def k (f x))
    (put groups k (array/push (or (get groups k) @[]) x)))
  groups)

(defn render
  [rows]
  (def out @"")
  (loop [row :in rows :when (next row)]
    (buffer/push-string out (string/join row " "))
    (buffer/push-string out "\n"))
  (string out))

(defn take-evens
  [ds]
  (seq [x :in ds :when (even? x)] x))

(defn normalize
  [p]
  (var cleaned (path/abspath p))
  (set cleaned (string/replace-all "\\" "/" cleaned))
  cleaned)

(defmacro bump [place]
  ~(set ,place (+ ,place 1)))
"#;

/// The same shapes, each broken in exactly one way.
const FENNEL_TWIN: &str = r#"(global registry {})

(fn count-keys [tbl]
  (var total 0)
  (each [_ _ {:a 1}]
    (print total))
  total)
"#;

const JANET_TWIN: &str = r#"(defn tally [ds]
  (var total 0)
  (loop [x :in ds])
  (put {:a 1} :b 2)
  total)
"#;

fn parse(source: &str, dialect: Dialect) -> SyntaxTree {
    SyntaxTree::parse_with_dialect(source, dialect).expect("the corpus must parse")
}

// -- the silent half ------------------------------------------------------

#[test]
fn correct_fennel_yields_no_findings() {
    assert_eq!(fired(FENNEL_CORPUS, Dialect::Fennel), Vec::<&str>::new());
}

#[test]
fn correct_janet_yields_no_findings() {
    assert_eq!(fired(JANET_CORPUS, Dialect::Janet), Vec::<&str>::new());
}

// -- and the denominator that makes it mean something ---------------------

#[test]
fn the_fennel_corpus_gives_every_rule_something_to_decline() {
    let tree = parse(FENNEL_CORPUS, Dialect::Fennel);
    assert_eq!(
        var_never_set::domain::candidate_count(Dialect::Fennel, &tree),
        3,
        "var bindings the rule looked at"
    );
    assert_eq!(
        fennel_each_over_non_iterator::domain::candidate_count(Dialect::Fennel, &tree),
        4,
        "each forms the rule looked at"
    );
    // `fennel-deprecated-form` is the one rule whose denominator *is* its
    // numerator: a deprecated special has no correct use, so correct code
    // contains none. Pinned at zero deliberately, and the twin below is what
    // proves the rule can fire at all.
    assert_eq!(
        fennel_deprecated_form::domain::candidate_count(Dialect::Fennel, &tree),
        0
    );
}

#[test]
fn the_janet_corpus_gives_every_rule_something_to_decline() {
    let tree = parse(JANET_CORPUS, Dialect::Janet);
    assert_eq!(
        var_never_set::domain::candidate_count(Dialect::Janet, &tree),
        2,
        "var bindings the rule looked at"
    );
    assert_eq!(
        janet_empty_loop_body::domain::candidate_count(Dialect::Janet, &tree),
        3,
        "loop/seq/catseq forms the rule looked at"
    );
    assert_eq!(
        janet_mutating_immutable_literal::domain::candidate_count(Dialect::Janet, &tree),
        4,
        "mutating calls the rule looked at"
    );
}

// -- the dangerous twin ---------------------------------------------------

#[test]
fn the_fennel_twin_fires_each_fennel_rule_exactly_once() {
    assert_eq!(
        fired(FENNEL_TWIN, Dialect::Fennel),
        vec![
            "fennel-deprecated-form",
            "fennel-each-over-non-iterator",
            "var-never-set"
        ]
    );
}

#[test]
fn the_janet_twin_fires_each_janet_rule_exactly_once() {
    assert_eq!(
        fired(JANET_TWIN, Dialect::Janet),
        vec![
            "janet-empty-loop-body",
            "janet-mutating-immutable-literal",
            "var-never-set"
        ]
    );
}

/// Reading the Fennel twin as Janet must leave exactly the one rule that is in
/// scope for both dialects, and drop the two Fennel-specific ones.
///
/// `(var total 0)` really is an unassigned Janet `var` as well, so asserting
/// silence here would be asserting a bug. What the dialect scope owes is that
/// `global` and the `each` binder — neither of which means anything in Janet —
/// stop being reported.
#[test]
fn the_fennel_twin_read_as_janet_keeps_only_the_shared_rule() {
    assert_eq!(fired(FENNEL_TWIN, Dialect::Janet), vec!["var-never-set"]);
}

/// And the reverse. The Janet twin's `(loop [x :in ds])` and
/// `(put {:a 1} :b 2)` are ordinary function calls in Fennel.
#[test]
fn the_janet_twin_read_as_fennel_keeps_only_the_shared_rule() {
    assert_eq!(fired(JANET_TWIN, Dialect::Fennel), vec!["var-never-set"]);
}
