//! The permanent corpus test: realistic correct Common Lisp yields nothing,
//! and a line-for-line dangerous twin trips every rule exactly once.
//!
//! # Why both halves, and why the candidate assertion
//!
//! A zero-findings sweep proves nothing on its own. A rule whose head never
//! matches, whose domain check is inverted, or which was quietly disabled
//! reports zero over any corpus at all — which is how a "clean" run has twice
//! been mistaken for a working one in this project. So [`CLEAN`] is asserted
//! *twice*: no findings, **and** a non-zero candidate count per rule, taken
//! from the same report's own denominator. The second assertion is what makes
//! the first mean something.
//!
//! [`DANGEROUS`] is the same code with one thing changed per rule. It fires
//! each rule exactly once, which pins the near-miss boundary from the other
//! side: the two files differ only in the details each rule claims to be about.
//!
//! # Where these came from
//!
//! Both are modelled on what the third-party audit actually found. Over SBCL
//! 2.6.0's own sources (670 files) and `~/quicklisp/dists` (698 files), these
//! rules produced **zero findings against non-zero denominators** — 73
//! `maphash` forms, 540 `make-array` forms, 452 `defstruct` forms, 854
//! `gethash`/`remhash` calls, 158 `vector-push` calls — and the seven
//! near-misses that got closest were adjudicated by hand as true negatives:
//!
//! - SBCL `compiler/vop.lisp:985`, `(:constructor make-random-tn (sc offset
//!   &aux (kind :normal)))` — an `&aux` *with* a value form, which is the
//!   option working correctly. [`CLEAN`] carries the same shape.
//! - mgl-pax `src/navigate/hyperspec.lisp:2065`, a literal `"s_lambda"` key on
//!   a table `let`-bound at line 2047 to `(make-hash-table :test #'equalp)` —
//!   resolved through the binding table and correctly left alone. [`CLEAN`]
//!   carries the same shape.
//! - alexandria `alexandria-1/tests.lisp:294-295`, literal `"FOO"` keys on
//!   tables built by `copy-hash-table`, which is not a construction this rule
//!   reads.

#![cfg(test)]

use std::path::Path;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;

/// Realistic, correct Common Lisp touching all six rules' domains.
///
/// Every construct here is the *permitted* neighbour of something a rule
/// reports, so a rule that widened by one step would break this file.
const CLEAN: &str = r#"
(defpackage #:inventory
  (:use #:common-lisp)
  (:export #:make-item #:record #:purge-expired))

(in-package #:inventory)

;;; A BOA constructor that omits two slots. Per CLHS both keep their
;;; :initform, so this is correct and must not be reported.
(defstruct (item (:constructor make-item (id name)))
  (id 0)
  (name "")
  (tags '())
  (weight 1.0))

;;; An &aux *with* a value form: the option working as designed. This is the
;;; shape SBCL's own compiler/vop.lisp:985 uses.
(defstruct (entry (:constructor make-entry (key &aux (stamp (get-universal-time)))))
  (key nil)
  (stamp 0))

;;; :include with matching representations, which CLHS permits.
(defstruct (base-record (:type list)) code label)
(defstruct (sized-record (:type list) (:include base-record)) bytes)

;;; And the ordinary untyped pair.
(defstruct plain-base a b)
(defstruct (plain-derived (:include plain-base)) c)

;;; An equal-tested table, so the literal key in FORGET resolves and matches.
(defparameter *index* (make-hash-table :test #'equal)
  "Maps a string name to an item.")

;;; A fill-pointered, adjustable buffer: what vector-push-extend requires.
(defparameter *buffer*
  (make-array 32 :element-type 'character :fill-pointer 0 :adjustable t))

(defun record (item)
  (setf (gethash (item-name item) *index*) item))

(defun lookup (name)
  (gethash name *index*))

(defun forget ()
  (remhash "last-recorded" *index*))

;;; remhash of the *current* key, which CLHS 18.2 explicitly permits; a setf of
;;; gethash on the current key, which it also permits; and mutation of a
;;; different table entirely, which is not a mid-walk mutation of this one.
(defun purge-expired (table archive)
  (maphash (lambda (key value)
             (cond ((expired-p value)
                    (setf (gethash key archive) value)
                    (remhash key table))
                   (t (setf (gethash key table) (touch value)))))
           table))

(defun collect-into (characters)
  (dolist (character characters *buffer*)
    (vector-push-extend character *buffer*)))

(defun fixed-buffer ()
  (let ((cells (make-array 16 :fill-pointer 0)))
    (vector-push 0 cells)
    cells))

(defun blank-grid (rows columns)
  (make-array (list rows columns) :initial-element 0))

(defun preset-row ()
  (make-array 3 :initial-contents '(1 2 3)))
"#;

/// [`CLEAN`], with exactly one thing broken per rule and everything else — in
/// particular every rule's candidate count — held identical.
const DANGEROUS: &str = r#"
(defpackage #:inventory
  (:use #:common-lisp)
  (:export #:make-item #:record #:purge-expired))

(in-package #:inventory)

(defstruct (item (:constructor make-item (id name)))
  (id 0)
  (name "")
  (tags '())
  (weight 1.0))

;;; BROKEN 1: a bare &aux naming a slot. The :initform 0 never runs, and
;;; reading the slot traps.
(defstruct (entry (:constructor make-entry (key &aux stamp)))
  (key nil)
  (stamp 0))

;;; BROKEN 2: the child declares no :type, the parent declares :type list.
(defstruct (base-record (:type list)) code label)
(defstruct (sized-record (:include base-record)) bytes)

(defstruct plain-base a b)
(defstruct (plain-derived (:include plain-base)) c)

;;; BROKEN 3: the default eql test, so the literal key in FORGET never matches.
(defparameter *index* (make-hash-table)
  "Maps a string name to an item.")

;;; BROKEN 4: :adjustable without :fill-pointer, which is the option people
;;; reach for and which vector-push-extend does not accept.
(defparameter *buffer*
  (make-array 32 :element-type 'character :adjustable t))

(defun record (item)
  (setf (gethash (item-name item) *index*) item))

(defun lookup (name)
  (gethash name *index*))

(defun forget ()
  (remhash "last-recorded" *index*))

;;; BROKEN 5: remhash of a key that is not the one being processed.
(defun purge-expired (table archive)
  (maphash (lambda (key value)
             (cond ((expired-p value)
                    (setf (gethash key archive) value)
                    (remhash sentinel table))
                   (t (setf (gethash key table) (touch value)))))
           table))

(defun collect-into (characters)
  (dolist (character characters *buffer*)
    (vector-push-extend character *buffer*)))

(defun fixed-buffer ()
  (let ((cells (make-array 16 :fill-pointer 0)))
    (vector-push 0 cells)
    cells))

(defun blank-grid (rows columns)
  (make-array (list rows columns) :initial-element 0))

;;; BROKEN 6: both initializers at once.
(defun preset-row ()
  (make-array 3 :initial-element 0 :initial-contents '(1 2 3)))
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

/// Every rule, as `(name, findings-on-clean, denominator-on-clean,
/// findings-on-dangerous)`.
fn sweep(source: &str) -> Vec<(&'static str, usize, u64)> {
    use crate::{
        defstruct_boa_aux_uninitialized_slot::domain::build_defstruct_boa_aux_uninitialized_slot_report as boa,
        defstruct_include_type_mismatch::domain::build_defstruct_include_type_mismatch_report as inc,
        hash_table_literal_string_key_under_eql::domain::build_hash_table_literal_string_key_report as key,
        make_array_conflicting_initializers::domain::build_make_array_conflicting_initializers_report as arr,
        maphash_mutates_other_entry::domain::build_maphash_mutates_other_entry_report as map,
        vector_push_without_fill_pointer::domain::build_vector_push_without_fill_pointer_report as push,
    };

    let (map_n, map_d) = measure!(map, "maphash_form_count", source);
    let (arr_n, arr_d) = measure!(arr, "make_array_form_count", source);
    let (boa_n, boa_d) = measure!(boa, "defstruct_form_count", source);
    let (inc_n, inc_d) = measure!(inc, "defstruct_form_count", source);
    let (key_n, key_d) = measure!(key, "keyed_accessor_count", source);
    let (push_n, push_d) = measure!(push, "push_form_count", source);

    vec![
        ("maphash-mutates-other-entry", map_n, map_d),
        ("make-array-conflicting-initializers", arr_n, arr_d),
        ("defstruct-boa-aux-uninitialized-slot", boa_n, boa_d),
        ("defstruct-include-type-mismatch", inc_n, inc_d),
        ("hash-table-literal-string-key-under-eql", key_n, key_d),
        ("vector-push-without-fill-pointer", push_n, push_d),
    ]
}

/// Correct code reports nothing — and every rule actually looked.
///
/// The denominator assertion is the load-bearing half. Without it a rule that
/// never matched its head, or whose domain check was inverted to always bail,
/// would pass this test while detecting nothing at all.
#[test]
fn realistic_correct_common_lisp_yields_no_findings_against_real_candidates() {
    for (rule, findings, denominator) in sweep(CLEAN) {
        assert_eq!(
            findings, 0,
            "{rule} fired on correct code — false positives are how these rules die"
        );
        assert!(
            denominator > 0,
            "{rule} scanned zero candidates, so its zero findings prove nothing; \
             the corpus no longer exercises this rule's domain"
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

/// The two files must stay comparable: a change that drops a construct from
/// one and not the other would let the pair pass while testing less.
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
            "{rule} sees a different number of candidates in the two files, so they are no \
             longer a matched pair"
        );
    }
}
