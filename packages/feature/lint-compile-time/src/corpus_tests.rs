//! The permanent corpus test: one file of realistic **correct** Common Lisp
//! that must produce nothing, and its dangerous twin that must fire each rule
//! exactly once.
//!
//! # Why the candidate count is asserted too
//!
//! "Zero findings" is worthless on its own. A rule whose head list is wrong, or
//! whose byte-scan guard is inverted, is silent on *everything* and passes a
//! zero-findings assertion perfectly. This repository has already been burned by
//! a sweep harness that errored out on every batch and reported a clean run.
//!
//! So the clean corpus asserts a **non-zero denominator** as well: the file
//! really does contain `eval-when` and `defconstant` forms, they really were
//! reached and examined, and the rules really did decline them.
//!
//! # Where the negative cases come from
//!
//! Not invented. Every shape in [`CORRECT`] is one the audit over 1588
//! third-party files (SBCL 2.6.0's own sources plus `~/quicklisp/dists/`)
//! actually produced, including the two that would be false positives without
//! the quote model:
//!
//! - a `defmacro` whose template emits `(eval-when (:compile-toplevel) …)` for
//!   the *caller's* file, where the situation is correct — 30 occurrences in the
//!   corpus, the shape at
//!   `closer-mop-20260101-git/closer-mop-shared.lisp:509`, and a false positive
//!   for `eval-when-body-never-runs` in any implementation that walks data;
//! - `(eval-when (:compile-toplevel :execute) …)` and
//!   `(eval-when (:load-toplevel :execute) …)`, both common and both correct.

use std::path::Path;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;

use crate::defconstant_non_eql_value::domain::build_defconstant_non_eql_value_report;
use crate::eval_when_body_never_runs::domain::build_eval_when_body_never_runs_report;
use crate::eval_when_execute_only::domain::build_eval_when_execute_only_report;

/// Realistic, idiomatic, **correct** Common Lisp. Every form here loads and
/// compiles identically; nothing in it is a phase mistake.
const CORRECT: &str = r#"
(defpackage #:inventory
  (:use #:cl)
  (:export #:reorder-level #:describe-item))

(in-package #:inventory)

;;; The full three situations: the ordinary spelling for a macro helper that the
;;; rest of the file expands against.
(eval-when (:compile-toplevel :load-toplevel :execute)
  (defun slot-reader-name (slot)
    (intern (format nil "~a-OF" (symbol-name slot)))))

(eval-when (:compile-toplevel :load-toplevel :execute)
  (defmacro define-reader (slot)
    `(defun ,(slot-reader-name slot) (item) (getf item ,(intern (symbol-name slot) :keyword)))))

(define-reader name)
(define-reader sku)

;;; :load-toplevel :execute, with no :compile-toplevel. Correct, and measured
;;; against SBCL 2.6.0 as behaving identically to the full three situations --
;;; defmacro's own expansion carries an inner (eval-when (:compile-toplevel) ..)
;;; and CLHS 3.2.3.1 keeps an eval-when body top level.
(eval-when (:load-toplevel :execute)
  (defmacro with-item ((var item) &body body)
    `(let ((,var ,item)) ,@body)))

;;; :compile-toplevel :execute, also common and also correct.
(eval-when (:compile-toplevel :execute)
  (defparameter *known-units* '(:each :case :pallet)))

;;; A macro whose TEMPLATE emits an eval-when for the caller's file. The
;;; situation list omits :execute and the form is lexically inside a defmacro
;;; body -- which is exactly `eval-when-body-never-runs`'s shape. It is data,
;;; not a form, and flagging it is a false positive. 30 occurrences of this
;;; shape appear in the audited corpus.
(defmacro define-both (name args &body body)
  `(progn
     (eval-when (:compile-toplevel)
       (cl:defgeneric ,name ,args))
     (eval-when (:load-toplevel :execute)
       (defun ,name ,args ,@body))))

;;; A nested eval-when naming :execute: correct, because outside a top level
;;; form the standard considers only that situation.
(defun rebuild-cache (force)
  (when force
    (eval-when (:execute)
      (clrhash *cache*)))
  t)

;;; Constants whose values are eql to themselves.
(defconstant +reorder-level+ 25)
(defconstant +max-sku-length+ 32)
(defconstant +default-unit+ :each)
(defconstant +unset+ nil)
(defconstant +pi-ish+ 3.14159d0)
(defconstant +separator+ #\-)

;;; The idiom for an aggregate constant. Its head is `define-constant`, not
;;; `defconstant`, so it is never even dispatched.
(alexandria:define-constant +unit-names+ #("each" "case" "pallet")
  :test #'equalp)

;;; A defconstant whose initform is a call this package does not model. Whether
;;; it conses is not a question the source answers, so nothing is said.
(defconstant +build-stamp+ (compute-build-stamp))

(defun describe-item (item)
  (with-item (i item)
    (format nil "~a (~a)" (name-of i) (sku-of i))))
"#;

/// The dangerous twin. Each rule must fire exactly once, and no rule may fire
/// on another's form.
const DANGEROUS: &str = r#"
(defpackage #:inventory-broken
  (:use #:cl))

(in-package #:inventory-broken)

;;; 1. eval-when-execute-only. Loads fine as source; compile-file discards the
;;;    body entirely and the fasl has no REORDER-P at all.
(eval-when (:execute)
  (defmacro reorder-p (level) `(< ,level +reorder-level+)))

;;; 2. defconstant-non-eql-value. A fresh vector each evaluation, so
;;;    compile-file + load of the fasl signals DEFCONSTANT-UNEQL on a first
;;;    build. Verified against SBCL 2.6.0 through asdf:load-system.
(defconstant +unit-names+ #("each" "case" "pallet"))

;;; 3. eval-when-body-never-runs. Not a top level form, so CLHS 3.2.3.1
;;;    considers only :execute here -- which is not named, so the body never
;;;    runs, in any phase, with no diagnostic from the compiler.
(defun warm-cache ()
  (eval-when (:compile-toplevel :load-toplevel)
    (setf *warmed* t))
  :done)
"#;

fn reports(source: &str) -> (usize, usize, usize, u64, u64) {
    let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse corpus");
    let path = Path::new("corpus.lisp");
    let execute_only = build_eval_when_execute_only_report(path, Dialect::CommonLisp, &tree)
        .expect("execute-only report");
    let ignored = build_eval_when_body_never_runs_report(path, Dialect::CommonLisp, &tree)
        .expect("body-never-runs report");
    let non_eql = build_defconstant_non_eql_value_report(path, Dialect::CommonLisp, &tree)
        .expect("non-eql report");

    let summary = |report: &[(&'static str, serde_json::Value)], key: &str| {
        report
            .iter()
            .find(|(name, _)| *name == key)
            .and_then(|(_, value)| value.as_u64())
            .unwrap_or_default()
    };

    (
        execute_only.findings.len(),
        ignored.findings.len(),
        non_eql.findings.len(),
        summary(&execute_only.summary, "eval_when_count"),
        summary(&non_eql.summary, "defconstant_count"),
    )
}

#[test]
fn realistic_correct_common_lisp_produces_no_findings() {
    let (execute_only, ignored, non_eql, _, _) = reports(CORRECT);
    assert_eq!(
        execute_only, 0,
        "eval-when-execute-only fired on correct code"
    );
    assert_eq!(
        ignored, 0,
        "eval-when-body-never-runs fired on correct code"
    );
    assert_eq!(
        non_eql, 0,
        "defconstant-non-eql-value fired on correct code"
    );
}

/// The half that makes the assertion above mean something. A rule that is
/// silent because it was never invoked passes a zero-findings test perfectly.
#[test]
fn the_correct_corpus_really_does_contain_candidates() {
    let (_, _, _, eval_whens, defconstants) = reports(CORRECT);
    assert!(
        eval_whens >= 5,
        "the clean corpus examined only {eval_whens} eval-when forms; a zero-findings result \
         over no candidates is a false clean"
    );
    assert!(
        defconstants >= 6,
        "the clean corpus examined only {defconstants} defconstant forms; a zero-findings result \
         over no candidates is a false clean"
    );
}

#[test]
fn the_dangerous_twin_fires_each_rule_exactly_once() {
    let (execute_only, ignored, non_eql, eval_whens, defconstants) = reports(DANGEROUS);
    assert_eq!(execute_only, 1, "eval-when-execute-only");
    assert_eq!(ignored, 1, "eval-when-body-never-runs");
    assert_eq!(non_eql, 1, "defconstant-non-eql-value");
    assert!(eval_whens >= 2 && defconstants >= 1);
}

/// The two corpora must differ only in the defect, not in which forms exist at
/// all: a twin that simply deleted the correct code would prove nothing about
/// the rules' ability to tell the two apart.
#[test]
fn both_corpora_exercise_both_heads() {
    for (label, source) in [("correct", CORRECT), ("dangerous", DANGEROUS)] {
        let (_, _, _, eval_whens, defconstants) = reports(source);
        assert!(eval_whens > 0, "{label} corpus has no eval-when candidate");
        assert!(
            defconstants > 0,
            "{label} corpus has no defconstant candidate"
        );
    }
}
