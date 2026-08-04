//! A false-positive corpus: realistic **correct** Common Lisp that uses every
//! head this rule anchors on, linted through the real `HeadFilter::Heads`
//! dispatch, paired with a dangerous twin that fires the rule exactly once.
//!
//! # Why this runs through the engine
//!
//! Calling `examine` directly bypasses the head index, which is where a wrong
//! `HeadFilter` shows up. A sibling agent in this repository deleted a head from
//! a rule and its whole suite stayed green for exactly that reason. Everything
//! here goes through [`collect_lint_pass`], so the dispatch is under test too.
//!
//! # Why the candidate count is asserted
//!
//! A zero-finding sweep over zero candidates is a **false-clean**. If the corpus
//! stopped mentioning `sort`, or the head list lost an entry, the findings would
//! still be zero and the test would still pass. So the corpus asserts a non-zero
//! number of nodes *handed to the rule* as well as zero findings, and the
//! per-head test below pins that the index hands over each of the six.

use std::path::Path;

use paredit_core_lint_engine::engine::{PassOptions, build_head_index, collect_lint_pass};
use paredit_core_lint_engine::policy::RuleSelection;
use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;

static ENTRIES: [RuleEntry; 1] = [RuleEntry::new(
    &crate::discarded_destructive_sequence_result::META,
    &crate::discarded_destructive_sequence_result::RULE,
)];

/// `(findings, candidate nodes handed to the rule)`.
fn lint(source: &str) -> (Vec<String>, u64) {
    let catalog = RuleCatalog::new(&ENTRIES);
    let index = build_head_index(catalog);
    let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
    let outcome = collect_lint_pass(
        catalog,
        &index,
        Path::new("corpus.lisp"),
        Dialect::CommonLisp,
        &tree,
        source,
        RuleSelection::All,
        PassOptions {
            settings: None,
            measure: true,
        },
    )
    .expect("lint pass");

    let candidates = outcome
        .timings
        .as_ref()
        .expect("measure: true produces timings")
        .entries()
        .map(|(_, _, invocations)| invocations)
        .sum();

    let findings = outcome
        .outcomes
        .into_iter()
        .map(|item| {
            let (finding, _) = item.into_parts();
            let line = source[..finding.span.start().get()].lines().count();
            let text = source
                .get(finding.span.start().get()..finding.span.end().get())
                .unwrap_or("")
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_owned();
            format!("line {line}: {text}")
        })
        .collect();

    (findings, candidates)
}

/// Correct Common Lisp, written the way the idiom is actually written.
///
/// Every one of the six heads appears, in the shapes that must **not** be
/// reported: the result bound with `setf`, returned as the body's value, passed
/// straight to another call, bound by a `let`, accumulated with `push`, and
/// applied to a freshly-built temporary.
const CORRECT: &str = r#"
;;;; A small inventory module, written correctly throughout.

(defpackage :inventory (:use :cl) (:export #:rank #:merge-lots #:trim))
(in-package :inventory)

(defun rank (items)
  "Order ITEMS by price. The destructive sort's result is bound, not dropped."
  (setf items (sort items #'< :key #'item-price))
  (dolist (item items)
    (format t "~&~A~%" (item-name item)))
  items)

(defun rank-stably (items)
  ;; The value of the body is the value of the call: nothing is discarded.
  (stable-sort (copy-seq items) #'string< :key #'item-name))

(defun merge-lots (primary secondary)
  ;; `nconc`'s result is what the function returns.
  (nconc primary secondary))

(defun merge-into (place lots)
  ;; Bound by a `let`, then read. This is the correct spelling.
  (let ((joined (nconc (copy-list place) lots)))
    (length joined)))

(defun trim (items)
  (setf items (nbutlast items))
  items)

(defun collect-trimmed (batches)
  (let ((out '()))
    (dolist (batch batches out)
      ;; An argument to `push`: the value is consumed.
      (push (nbutlast (copy-list batch)) out))))

(defun rewrite-tree (tree)
  (setf tree (nsubst 'new 'old tree))
  (setf tree (nsublis '((a . 1) (b . 2)) tree))
  tree)

(defun normalize (name)
  ;; In-place by specification: discarding the result here is correct, and this
  ;; rule must not report it. See the README's SBCL table.
  (let ((copy (copy-seq name)))
    (nstring-downcase copy)
    copy))

(defun apply-defaults (row defaults)
  ;; `replace` and `nsubstitute` likewise rewrite in place.
  (replace row defaults)
  (nsubstitute 0 nil row)
  row)

(defun cleanup (items)
  ;; The heads SBCL already warns about are absent from this rule entirely,
  ;; correct spelling or not.
  (setf items (delete-duplicates items))
  (setf items (nreverse items))
  items)

(defmacro with-ranked ((var items) &body body)
  ;; A template that *writes* a sort is data, not a discarded call.
  `(let ((,var (sort (copy-list ,items) #'<)))
     ,@body))

(defun report (items)
  ;; Last form of the body: the value is the function's result.
  (sort (copy-list items) #'<))
"#;

/// The same module's dangerous twin: one genuine defect, and nothing else.
const DANGEROUS: &str = r#"
(defpackage :inventory (:use :cl))
(in-package :inventory)

(defun rank (items)
  "The bug: the sorted list is returned and dropped, and ITEMS is then read."
  (sort items #'< :key #'item-price)
  (dolist (item items)
    (format t "~&~A~%" (item-name item)))
  items)
"#;

#[test]
fn realistic_correct_code_yields_no_findings_over_a_real_denominator() {
    let (findings, candidates) = lint(CORRECT);
    assert!(
        candidates > 0,
        "the corpus handed the rule zero candidate nodes; a zero-finding sweep over zero \
         candidates proves nothing"
    );
    assert!(
        findings.is_empty(),
        "correct Common Lisp must yield no findings, got {} over {candidates} candidates: \
         {findings:#?}",
        findings.len()
    );
}

/// The discriminating half. Without this, the corpus above could be passing
/// because the rule never fires at all.
#[test]
fn the_dangerous_twin_fires_exactly_once() {
    let (findings, candidates) = lint(DANGEROUS);
    assert!(candidates > 0, "the twin must reach the rule");
    assert_eq!(
        findings.len(),
        1,
        "the dangerous twin must fire exactly once, got: {findings:#?}"
    );
    assert!(
        findings[0].contains("(sort items #'< :key #'item-price)"),
        "the finding must point at the discarded call, got: {}",
        findings[0]
    );
}

/// The head index must actually hand the rule each of the six heads.
///
/// This is the test that a deleted `Heads` entry fails. Asserting only "the
/// suite is green" would not: a head the index never dispatches produces no
/// findings, which looks exactly like correct code.
#[test]
fn the_head_index_dispatches_every_declared_head() {
    for (head, source) in [
        ("sort", "(defun f (xs) (sort xs #'<) (print xs))"),
        (
            "stable-sort",
            "(defun f (xs) (stable-sort xs #'<) (print xs))",
        ),
        ("nconc", "(defun f (xs ys) (nconc xs ys) (print xs))"),
        ("nbutlast", "(defun f (xs) (nbutlast xs) (print xs))"),
        ("nsublis", "(defun f (al xs) (nsublis al xs) (print xs))"),
        ("nsubst", "(defun f (xs) (nsubst 1 2 xs) (print xs))"),
    ] {
        let (findings, candidates) = lint(source);
        assert!(
            candidates > 0,
            "the head index handed the rule nothing for `{head}`: the Heads entry is missing"
        );
        assert_eq!(
            findings.len(),
            1,
            "`{head}` must produce exactly one finding through the engine, got {findings:#?}"
        );
    }
}

/// A body form inside quoted data must be declined by `check()` itself.
///
/// This test exists because of mutation testing: removing the `is_unevaluated_at`
/// suppression from `check()` broke **no** test. The unit tests all drive
/// `examine` through `for_each_evaluated_subview`, which filters data *before*
/// the rule ever sees it — so they can never exercise the suppression. The head
/// index has no such filter: it dispatches a `progn` whether or not a quote
/// encloses it, and `check()` is the only thing standing between that and a
/// false positive.
///
/// Each source below puts a genuine defect inside data. The engine must report
/// none of them, and the control at the end must still report one.
#[test]
fn a_body_form_inside_quoted_data_is_declined_by_the_rule_itself() {
    for source in [
        // A macro that returns a list which happens to look like code.
        "(defun template () '(progn (sort xs #'<) (print xs)))",
        "(defun template () (quote (progn (sort xs #'<) (print xs))))",
        // A backquoted template with no unquote is still data.
        "(defmacro m () `(progn (sort xs #'<) (print xs)))",
        // Nested a level deeper inside the quoted structure.
        "(defun template () '(a (defun f (xs) (sort xs #'<) (print xs))))",
    ] {
        let (findings, candidates) = lint(source);
        assert!(
            candidates > 0,
            "the head index must still dispatch the quoted body form in: {source}"
        );
        assert!(
            findings.is_empty(),
            "a body form inside quoted data must be declined, got {findings:#?} for: {source}"
        );
    }

    // The control: a comma re-enters code, and there the defect is real.
    let (findings, _) = lint("(defmacro m (&body b) `(progn ,(progn (sort xs #'<) (print xs))))");
    assert_eq!(
        findings.len(),
        1,
        "an unquoted body form is code again and must still report, got {findings:#?}"
    );
}

/// The correct idiom, through the engine rather than through `examine`.
#[test]
fn the_setf_idiom_is_dispatched_and_declined() {
    let (findings, candidates) = lint("(defun f (xs) (setf xs (sort xs #'<)) (print xs))");
    assert!(
        candidates > 0,
        "the `sort` node must still be dispatched — the rule declines it, the index does not"
    );
    assert!(
        findings.is_empty(),
        "the correct idiom must never be reported, got {findings:#?}"
    );
}
