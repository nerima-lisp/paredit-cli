//! The permanent corpus test: realistic correct macro-heavy Common Lisp yields
//! nothing, and a line-for-line dangerous twin trips every rule exactly once.
//!
//! # Why both halves, and why the candidate assertion
//!
//! A zero-findings sweep proves nothing on its own. A rule whose head never
//! matches, whose domain check is inverted, or which was quietly disabled
//! reports zero over any corpus at all. So [`CLEAN`] is asserted **twice**: no
//! findings, *and* a non-zero candidate count per rule, taken from the same
//! report's own denominator. The second assertion is what makes the first mean
//! something.
//!
//! [`DANGEROUS`] is the same code with one thing changed per rule. It fires
//! each rule exactly once, which pins the near-miss boundary from the other
//! side: the two files differ only in the details each rule claims to be about.
//!
//! # Where these came from
//!
//! Both are modelled on what the third-party audit actually found. Over SBCL
//! 2.6.6's own sources and `~/quicklisp/dists/quicklisp/software` — **1297
//! files, 2124 `defmacro`/`define-compiler-macro` candidates and 966 `macrolet`
//! candidates** — these two rules produced **one** finding, and it was a true
//! positive:
//!
//! - SBCL `src/compiler/type-vop-macros.lisp:181`,
//!   `(remf other-args :value-tn-ref)` inside
//!   `(defmacro test-type (… &rest other-args &key &allow-other-keys) …)`,
//!   whose result is then spliced back with `,@other-args`. Confirmed against
//!   SBCL 2.6.0: `remf` of a **non-first** key splices the caller's list, so
//!   `(test-type v t tg np :foo 1 :value-tn-ref 2 :bar 3)` becomes
//!   `(test-type v t tg np :foo 1 :bar 3)` in place. The control — `remf` of
//!   the *first* key — leaves it alone, which is why this has never bitten.
//!
//! The three findings that were **not** true positives are the reason two
//! guards exist, and both shapes are carried in [`CLEAN`] so that removing
//! either guard breaks this file:
//!
//! - SBCL `src/code/stream.lisp:1363` and `:1430`, `(case (dsd-name dsd)
//!   ((index start) 'start) …)` — a **case clause key list**, which is
//!   unevaluated and carries no quote character to say so.
//! - SBCL `src/code/type.lisp:2810`, `(loop for (class format coerce) in specs
//!   …)` inside a `macrolet` expander in a `defun` whose parameter is also
//!   called `format` — a **`loop` destructuring variable** that shadows it.

#![cfg(test)]

use std::path::Path;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;

/// Realistic, correct macro-heavy Common Lisp touching both rules' domains.
///
/// Every construct here is the *permitted* neighbour of something a rule
/// reports, so a rule that widened by one step would break this file.
const CLEAN: &str = r#"
(defpackage #:emitter
  (:use #:common-lisp)
  (:export #:with-buffer #:define-op))

(in-package #:emitter)

;;; A &body macro that reverses a *local accumulator*, which is the commonest
;;; correct use of NREVERSE there is.
(defmacro with-buffer ((name size) &body forms)
  (let ((out '()))
    (dolist (form forms) (push form out))
    `(let ((,name (make-array ,size :fill-pointer 0)))
       ,@(nreverse out))))

;;; The argument is copied first, so destroying it harms nobody.
(defmacro in-order (&body forms)
  `(progn ,@(nreverse (copy-list forms))))

;;; The parameter is rebound to a fresh list before being destroyed.
(defmacro sorted-clauses (&body clauses)
  (let ((clauses (copy-list clauses)))
    `(cond ,@(sort clauses #'string< :key #'princ-to-string))))

;;; NCONC whose *last* argument is the parameter: the tail is linked to, never
;;; destroyed.
(defmacro append-forms (&body forms)
  `(progn ,@(nconc (list '(setup)) forms)))

;;; A defaultable parameter holds a form the expander may have built itself.
(defmacro with-limit (a &optional (extras (list 1)))
  (nreverse extras)
  `(identity ,a))

;;; A compiler macro congruent with its function, destroying nothing.
(define-compiler-macro scale (a b) `(* ,a ,b))
(defun scale (a b) (* a b))

;;; The commonest MACROLET idiom: the captured-looking name is in the
;;; TEMPLATE, so it is part of the expansion and is bound where it lands.
(defun collect-ops (items)
  (let ((code '()))
    (macrolet ((emit (op) `(push ,op code)))
      (dolist (item items) (emit item))
      (nreverse code))))

;;; An expander that uses only its own parameters.
(defun double-all (values)
  (let ((n 2))
    (macrolet ((twice (x) `(* ,x 2)))
      (mapcar (lambda (v) (twice v)) (list n values)))))

;;; SBCL src/code/stream.lisp:1363 in miniature: `start` is a CASE CLAUSE KEY,
;;; not a variable reference, and nothing in the syntax says so.
(defun init-input-stream (stream string &optional (start 0) end)
  (macrolet ((initforms ()
               `(progn
                  ,@(mapcar (lambda (dsd)
                              `(%instance-set stream ,(dsd-index dsd)
                                              ,(case (dsd-name dsd)
                                                 ((index start) 'start)
                                                 (limit 'end)
                                                 (t (dsd-default dsd)))))
                            (dd-slots (find-defstruct-description 'string-input-stream))))))
    (initforms)
    (list stream string start end)))

;;; SBCL src/code/type.lisp:2810 in miniature: the LOOP clause rebinds
;;; `format`, which the enclosing defun also has as a parameter.
(defun make-union-type (class format complexp)
  (macrolet ((unionize (&rest specs)
               `(type-union
                 ,@(loop for (class format coerce) in specs
                         collect `(make-numeric-union-type
                                   :class ',class :format ',format :coerce ',coerce)))))
    (unionize (integer nil nil))
    (list class format complexp)))
"#;

/// [`CLEAN`], with exactly one thing broken per rule and everything else — in
/// particular every rule's candidate count — held identical.
const DANGEROUS: &str = r#"
(defpackage #:emitter
  (:use #:common-lisp)
  (:export #:with-buffer #:define-op))

(in-package #:emitter)

;;; BROKEN 1: the accumulator is gone; NREVERSE now destroys the caller's own
;;; &body list, so the second expansion of any call site sees a shorter one.
(defmacro with-buffer ((name size) &body forms)
  (let ((out '()))
    (dolist (form forms) (push form out))
    `(let ((,name (make-array ,size :fill-pointer 0)))
       ,@(nreverse forms))))

(defmacro in-order (&body forms)
  `(progn ,@(nreverse (copy-list forms))))

(defmacro sorted-clauses (&body clauses)
  (let ((clauses (copy-list clauses)))
    `(cond ,@(sort clauses #'string< :key #'princ-to-string))))

(defmacro append-forms (&body forms)
  `(progn ,@(nconc (list '(setup)) forms)))

(defmacro with-limit (a &optional (extras (list 1)))
  (nreverse extras)
  `(identity ,a))

(define-compiler-macro scale (a b) `(* ,a ,b))
(defun scale (a b) (* a b))

(defun collect-ops (items)
  (let ((code '()))
    (macrolet ((emit (op) `(push ,op code)))
      (dolist (item items) (emit item))
      (nreverse code))))

;;; BROKEN 2: `n` is now under a comma, so the expander reads the enclosing
;;; LET's variable at macroexpansion time, before it exists.
(defun double-all (values)
  (let ((n 2))
    (macrolet ((twice (x) `(* ,x ,n)))
      (mapcar (lambda (v) (twice v)) (list n values)))))

(defun init-input-stream (stream string &optional (start 0) end)
  (macrolet ((initforms ()
               `(progn
                  ,@(mapcar (lambda (dsd)
                              `(%instance-set stream ,(dsd-index dsd)
                                              ,(case (dsd-name dsd)
                                                 ((index start) 'start)
                                                 (limit 'end)
                                                 (t (dsd-default dsd)))))
                            (dd-slots (find-defstruct-description 'string-input-stream))))))
    (initforms)
    (list stream string start end)))

(defun make-union-type (class format complexp)
  (macrolet ((unionize (&rest specs)
               `(type-union
                 ,@(loop for (class format coerce) in specs
                         collect `(make-numeric-union-type
                                   :class ',class :format ',format :coerce ',coerce)))))
    (unionize (integer nil nil))
    (list class format complexp)))
"#;

fn parse(source: &str) -> SyntaxTree {
    SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("the corpus must parse")
}

/// Runs one rule's report builder over `source`, returning
/// `(findings, denominator)`.
macro_rules! measure {
    ($build:path, $summary:literal, $source:expr) => {{
        let tree = parse($source);
        let report = $build(Path::new("corpus.lisp"), Dialect::CommonLisp, &tree)
            .expect("the report must build");
        let denominator = report
            .summary
            .iter()
            .find(|(name, _)| *name == $summary)
            .and_then(|(_, value)| value.as_u64())
            .expect("the report must carry its denominator");
        (report.findings.len(), denominator)
    }};
}

/// Every rule, as `(name, findings, denominator)`.
fn sweep(source: &str) -> Vec<(&'static str, usize, u64)> {
    use crate::{
        macro_body_destroys_argument_form::domain::build_macro_body_destroys_argument_form_report as destroys,
        macrolet_expander_captures_lexical_variable::domain::build_macrolet_expander_captures_lexical_variable_report as captures,
    };

    let (destroys_n, destroys_d) = measure!(destroys, "macro_definition_count", source);
    let (captures_n, captures_d) = measure!(captures, "macrolet_form_count", source);

    vec![
        ("macro-body-destroys-argument-form", destroys_n, destroys_d),
        (
            "macrolet-expander-captures-lexical-variable",
            captures_n,
            captures_d,
        ),
    ]
}

/// Correct code reports nothing — and every rule actually looked.
///
/// The denominator assertion is the load-bearing half. Without it a rule that
/// never matched its head, or whose domain check was inverted to always bail,
/// would pass this test while detecting nothing at all.
#[test]
fn realistic_correct_macro_authoring_yields_no_findings_against_real_candidates() {
    for (rule, findings, denominator) in sweep(CLEAN) {
        assert_eq!(
            findings, 0,
            "{rule} fired on correct code — false positives are how these rules die"
        );
        assert!(
            denominator > 0,
            "{rule} scanned zero candidates, so its zero findings prove nothing; the corpus no \
             longer exercises this rule's domain"
        );
    }
}

/// The dangerous twin trips every rule exactly once.
#[test]
fn the_dangerous_twin_trips_every_rule_exactly_once() {
    for (rule, findings, denominator) in sweep(DANGEROUS) {
        assert_eq!(findings, 1, "{rule} should fire exactly once on the twin");
        assert!(denominator > 0, "{rule} scanned zero candidates");
    }
}

/// The two files must stay comparable: a change that drops a construct from one
/// and not the other would let the pair pass while testing less.
#[test]
fn the_two_corpus_files_scan_the_same_candidates() {
    let clean: Vec<_> = sweep(CLEAN)
        .into_iter()
        .map(|(rule, _, denominator)| (rule, denominator))
        .collect();
    let dangerous: Vec<_> = sweep(DANGEROUS)
        .into_iter()
        .map(|(rule, _, denominator)| (rule, denominator))
        .collect();
    for ((rule, clean_count), (_, dangerous_count)) in clean.iter().zip(&dangerous) {
        assert_eq!(
            clean_count, dangerous_count,
            "{rule} sees a different number of candidates in the two files, so they are no longer \
             a matched pair"
        );
    }
}

/// The same pair, driven through the **engine** rather than the report
/// builders, so the `HeadFilter::Heads` declarations are exercised too.
#[test]
fn the_corpus_pair_behaves_the_same_through_the_dispatcher() {
    assert_eq!(
        crate::engine_pass_tests::fired_names(CLEAN, Dialect::CommonLisp),
        Vec::<&str>::new(),
        "the clean corpus must be clean through the dispatcher as well"
    );
    let mut fired = crate::engine_pass_tests::fired_names(DANGEROUS, Dialect::CommonLisp);
    fired.sort_unstable();
    assert_eq!(
        fired,
        vec![
            "macro-body-destroys-argument-form",
            "macrolet-expander-captures-lexical-variable",
        ]
    );
}
