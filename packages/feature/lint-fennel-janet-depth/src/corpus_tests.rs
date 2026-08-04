//! A permanent corpus per dialect: realistic *correct* code that must stay
//! silent, and a dangerous twin that must fire every rule exactly once.
//!
//! The silent half is worthless on its own. A rule whose head is misspelled,
//! whose dialect scope is wrong, or that was deleted outright also produces
//! zero findings here — so each corpus asserts a **candidate count** as well,
//! taken from the same `candidate_count` the third-party audit used. A
//! zero-finding sweep over zero candidates says nothing; a zero-finding sweep
//! over dozens of candidates says the rules looked and declined.
//!
//! The idioms are taken from the code the audit actually ran over: the Fennel
//! corpus follows `Olical/conjure`, `rktjmp/hotpot.nvim` and Fennel's own
//! `src/fennel`, and the Janet corpus follows `janet-lang/spork` and `jpm` —
//! a PEG grammar in a quasiquoted struct, a `match` with a trailing default,
//! accumulate into a buffer.

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;

use crate::engine_pass_tests::fired;
use crate::{
    fennel_bad_unpack, fennel_nested_associative_operator, fennel_redundant_do,
    janet_dead_branch_on_constant_condition, janet_unreachable_match_clause,
};

/// Correct, idiomatic Fennel. Every rule in this package must decline it.
const FENNEL_CORPUS: &str = r#";; A small module in the style of conjure and hotpot.
(local {: view} (require :fennel.view))
(import-macros {: when-some} :my.macros)

(fn render-all [items sep]
  "Concatenate every rendered item, the variadic way."
  (let [out []]
    (each [_ item (ipairs items)]
      (table.insert out (view item)))
    (table.concat out sep)))

(fn describe [path opts]
  ;; A flat `..` with four arguments, not three nested two-argument ones.
  (.. "loading " path " as " (or opts.name "?")))

(fn sum [xs]
  ;; `accumulate` takes exactly one body expression, so no `do` is redundant.
  (accumulate [total 0 _ n (ipairs xs)] (+ total n)))

(fn first-index [xs pred]
  (accumulate [found nil i x (ipairs xs) &until found]
    (if (pred x) i found)))

(fn valid? [entry]
  ;; Mixed operators do not nest into themselves.
  (and entry (or entry.name entry.path) (not entry.hidden)))

(fn shift [flags n]
  (bor (band flags 255) (lshift n 8)))

(fn read-lines [handle]
  (let [acc []]
    (each [line (handle:lines)]
      (table.insert acc line))
    acc))

(fn write-config [path config]
  (with-open [out (io.open path :w)]
    (out:write (view config))
    (out:close)))

(fn tally [xs]
  (var running 0)
  (for [i 1 (length xs)]
    (set running (+ running (. xs i))))
  running)

;; A macro whose body is a template: the `do` is what lets it return two forms.
(macro twice [expr]
  `(do ,expr ,expr))

;; And one that splices a list into the tail of a call, which is legal exactly
;; because a real function call is variadic in its final argument.
(macro all-of [conditions]
  `(and ,(unpack conditions)))

{: render-all : describe : sum : first-index : valid? : shift : read-lines
 : write-config : tally}
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

(defn render
  "Accumulate into a buffer, the idiomatic way."
  [items]
  (def out @"")
  (each item items
    (buffer/push-string out (string item))
    (buffer/push-string out "\n"))
  (string out))

(defn classify
  "A match whose only catch-all is the last pattern — the default clause."
  [code]
  (match code
    404 :not-found
    500 :server-error
    [:redirect target] target
    {:kind k} k
    _ :unknown))

(defn describe
  "A match with guards, which are tuple patterns and never catch-alls."
  [value]
  (match value
    (n (> n 100)) :big
    (n (> n 10)) :medium
    :small))

# `if-not` is a PEG combinator here, not a conditional, and the grammar is a
# quasiquoted struct.
(def sep-peg
  (peg/compile
    ~{:segment (some (if-not (set "/\\") 1))
      :main (any (+ :segment (set "/\\")))}))

(defn resolve-mode
  "Conditionals whose tests are real expressions."
  [opts]
  (cond
    (opts :verbose) :verbose
    (dyn :quiet) :quiet
    :default))

(defn maybe-warn [opts]
  (unless (opts :silent)
    (eprint "working"))
  (when (opts :trace)
    (eprint "tracing"))
  (if (opts :strict) :strict :lenient))

(defn walk-up [start]
  (var current start)
  (while (not (empty? current))
    (set current (path/dirname current)))
  current)

(defn pick [opts fallback]
  # Two more real-expression conditionals, so the denominator is not one
  # unlucky edit away from zero.
  (if-not (opts :name)
    fallback
    (if (string? (opts :name)) (opts :name) (string (opts :name)))))

{:tally tally :render render :classify classify :describe describe
 :resolve-mode resolve-mode :maybe-warn maybe-warn :walk-up walk-up}
"#;

/// The dangerous twin: one instance of each Fennel defect and nothing else.
const FENNEL_DANGEROUS: &str = r#"(fn report [items prefix]
  ;; 1. `..` is not variadic at runtime, so only the first value survives.
  (print (.. prefix (table.unpack items)))
  ;; 2. a nested `or` that can be spliced into its parent.
  (when (or items.a (or items.b items.c))
    ;; 3. a `do` in the tail of a form that already sequences its body.
    (do (print :found) (print :done))))
"#;

/// The dangerous twin for Janet.
const JANET_DANGEROUS: &str = r#"(defn report [code]
  # 1. a constant test, so the else branch can never run.
  (if true (print "always") (print "never"))
  # 2. a catch-all pattern with a live-looking clause after it.
  (match code
    x :caught-everything
    404 :not-found))
"#;

// -- the silent half ------------------------------------------------------

#[test]
fn correct_fennel_produces_no_findings() {
    assert_eq!(
        fired(FENNEL_CORPUS, Dialect::Fennel),
        Vec::<&str>::new(),
        "a rule fired on idiomatic Fennel"
    );
}

#[test]
fn correct_janet_produces_no_findings() {
    assert_eq!(
        fired(JANET_CORPUS, Dialect::Janet),
        Vec::<&str>::new(),
        "a rule fired on idiomatic Janet"
    );
}

// -- and the denominators that make the silence mean something ------------

/// Without this, deleting every rule in the package leaves the two tests above
/// green.
#[test]
fn the_fennel_corpus_really_does_contain_candidates_for_every_rule() {
    let tree = SyntaxTree::parse_with_dialect(FENNEL_CORPUS, Dialect::Fennel).expect("parse");
    let dialect = Dialect::Fennel;

    let unpack = fennel_bad_unpack::domain::candidate_count(dialect, &tree);
    let nested = fennel_nested_associative_operator::domain::candidate_count(dialect, &tree);
    let redundant = fennel_redundant_do::domain::candidate_count(dialect, &tree);

    // Lower bounds rather than equalities: a later edit that adds an idiom
    // should not have to update a number, but one that guts the corpus fails.
    assert!(unpack >= 5, "bad-unpack candidates: {unpack}");
    assert!(nested >= 5, "nested-operator candidates: {nested}");
    assert!(redundant >= 15, "redundant-do candidates: {redundant}");
}

#[test]
fn the_janet_corpus_really_does_contain_candidates_for_every_rule() {
    let tree = SyntaxTree::parse_with_dialect(JANET_CORPUS, Dialect::Janet).expect("parse");
    let dialect = Dialect::Janet;

    let dead = janet_dead_branch_on_constant_condition::domain::candidate_count(dialect, &tree);
    let clause = janet_unreachable_match_clause::domain::candidate_count(dialect, &tree);

    assert!(dead >= 5, "dead-branch candidates: {dead}");
    assert!(clause >= 2, "match candidates: {clause}");
}

// -- the dangerous twin ---------------------------------------------------

#[test]
fn the_dangerous_fennel_twin_fires_every_fennel_rule_exactly_once() {
    assert_eq!(
        fired(FENNEL_DANGEROUS, Dialect::Fennel),
        vec![
            "fennel-bad-unpack",
            "fennel-nested-associative-operator",
            "fennel-redundant-do"
        ]
    );
}

#[test]
fn the_dangerous_janet_twin_fires_every_janet_rule_exactly_once() {
    assert_eq!(
        fired(JANET_DANGEROUS, Dialect::Janet),
        vec![
            "janet-dead-branch-on-constant-condition",
            "janet-unreachable-match-clause"
        ]
    );
}

/// The twins are the same *language* as the corpora, so a rule cannot pass the
/// silent half by being scoped to the wrong dialect and the dangerous half by
/// accident. Crossing them over must produce nothing at all.
#[test]
fn neither_dangerous_twin_fires_on_the_other_dialect() {
    assert_eq!(fired(FENNEL_DANGEROUS, Dialect::Janet), Vec::<&str>::new());
    assert_eq!(fired(JANET_DANGEROUS, Dialect::Fennel), Vec::<&str>::new());
}

/// Each corpus must also survive being read as the *other* dialect without a
/// finding, which is what a misconfigured `--dialect` would do to a user.
#[test]
fn neither_corpus_fires_when_read_as_the_other_dialect() {
    assert_eq!(fired(FENNEL_CORPUS, Dialect::Janet), Vec::<&str>::new());
    assert_eq!(fired(JANET_CORPUS, Dialect::Fennel), Vec::<&str>::new());
}
