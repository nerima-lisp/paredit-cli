//! The permanent regression corpus for `unused-local-binding`.
//!
//! Two files, and both halves matter. [`CORRECT`] is realistic Common Lisp
//! that must yield **zero** findings — every shape that made a real-world
//! false positive during the audit over quicklisp and SBCL is in it. But a
//! zero-finding sweep over zero candidates proves nothing, so the test also
//! asserts the candidate count is non-zero: a rule that silently stopped
//! matching anything would pass the first assertion and fail the second.
//!
//! [`DANGEROUS`] is its twin, differing only where the defect is, and each of
//! its bindings must be reported exactly once.

/// Realistic, correct Common Lisp. Zero findings, non-zero candidates.
pub const CORRECT: &str = r#"
(defpackage :corpus (:use :cl))
(in-package :corpus)

(defvar *registry* nil)

;; Every binding read in the body.
(defun plain (items limit)
  (let ((count (length items))
        (cap (or limit 10)))
    (list count cap)))

;; let* reading each previous clause.
(defun sequential (text)
  (let* ((trimmed (string-trim " " text))
         (size (length trimmed))
         (half (floor size 2)))
    (list trimmed size half)))

;; The author declared it unused; saying so again is noise.
(defun declared-ignore (a b)
  (let ((unused-value (compute a)))
    (declare (ignore unused-value))
    b))

(defun declared-ignorable (a)
  (let ((maybe (compute a)))
    (declare (ignorable maybe))
    a))

;; The conventional spelling of a deliberately unused name.
(defun underscore-name (items)
  (let ((_scratch (length items)))
    items))

;; A dynamic rebinding is read by callees with no textual reference. Both the
;; bare and the package-qualified spelling: eight of the audit's first twenty
;; findings were the qualified one.
(defun dynamic-rebind (stream)
  (let ((*registry* nil))
    (write-string "x" stream)))

(defun qualified-dynamic-rebind (value)
  (let ((sb-debug:*stack-top-hint* value))
    (invoke-debugger nil)))

;; A local function reached only through `#'`, which the binding table drops.
(defun function-quoted (items)
  (flet ((double (x) (* 2 x)))
    (mapcar #'double items)))

(defun function-designator-long-hand (items)
  (flet ((triple (x) (* 3 x)))
    (mapcar (function triple) items)))

;; A local function called normally, and a mutually recursive pair.
(defun called-normally (n)
  (labels ((even-p (k) (if (zerop k) t (odd-p (1- k))))
           (odd-p (k) (if (zerop k) nil (even-p (1- k)))))
    (even-p n)))

;; A variable spliced into operator position of a macro template resolves in
;; the function namespace and is missed by the table.
(defmacro spliced-operator (type)
  (let ((hasher (intern (format nil "HASH-~A" type))))
    `(,hasher key)))

;; A variable spliced into a template argument.
(defmacro spliced-argument ()
  (let ((index '#:index))
    `(loop for ,index from 1 to 10 collect ,index)))

;; A name read only from inside a reader conditional, which the dialect-aware
;; parse folds into a single opaque atom.
(defun reader-conditional (key)
  (let ((table (make-hash-table)))
    #+sbcl (gethash key table)
    #-sbcl nil))

;; An unknown macro in scope may expand into a reference.
(defun unknown-macro-in-scope (value)
  (let ((captured value))
    (some-project-macro)))

;; Reassigned. Never reported, because the table records a reference at the
;; assignment site — see the note on `suppression`.
(defun assigned-not-read (items)
  (let ((accumulator nil))
    (dolist (item items)
      (setf accumulator item))))

;; A binding read only from a nested scope.
(defun read-from-nested-scope (a)
  (let ((outer (compute a)))
    (let ((inner (1+ a)))
      (list outer inner))))

;; A binder inside a macro template binds nothing this layer records.
(defmacro template ()
  `(let ((generated 1))
     (list 2)))
"#;

/// The same shapes with the defect present. Each binding is reported once.
pub const DANGEROUS: &str = r#"
(defpackage :corpus (:use :cl))
(in-package :corpus)

;; A plain unread let binding.
(defun plain-unused (items)
  (let ((count (length items))
        (dead-one (length items)))
    count))

;; An unread let* binding.
(defun sequential-unused (text)
  (let* ((trimmed (string-trim " " text))
         (dead-two (length trimmed)))
    trimmed))

;; An unread local function.
(defun flet-unused (items)
  (flet ((dead-three (x) (* 2 x)))
    (length items)))

;; A local function shadowed in the value namespace only: `(list x)` reads the
;; *variable*, so the `flet` binding is still unread. This is the Lisp-2 fact
;; a textual scan gets wrong.
(defun lisp-two-unused (dead-four)
  (flet ((dead-four () 1))
    (list dead-four)))
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unused_local_binding::domain::{Outcome, examine};
    use paredit_core_lint_engine::engine::RuleContext;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;
    use paredit_core_syntax::view_query::for_each_subview;
    use std::path::Path;

    fn run(source: &str) -> Outcome {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        let context =
            RuleContext::new(Path::new("corpus.lisp"), Dialect::CommonLisp, &tree, source);
        let mut total = Outcome::default();
        let root = tree.root_view();
        for child in &root.children {
            for_each_subview(child, |view| {
                let outcome = examine(&context, view, true);
                total.candidates += outcome.candidates;
                total.findings.extend(outcome.findings);
                total.suppressed.extend(outcome.suppressed);
            });
        }
        total
    }

    /// Realistic correct code yields nothing.
    #[test]
    fn correct_common_lisp_yields_no_findings() {
        let outcome = run(CORRECT);
        let names: Vec<&str> = outcome
            .findings
            .iter()
            .map(|finding| finding.name.as_str())
            .collect();
        assert!(names.is_empty(), "false positives: {names:?}");
    }

    /// …and the zero above is a real zero, not the zero of a rule that has
    /// quietly stopped matching anything. This is the assertion that catches a
    /// `HeadFilter` typo or a pre-filter that rejects everything.
    #[test]
    fn the_correct_corpus_presents_a_non_zero_candidate_count() {
        let outcome = run(CORRECT);
        assert!(
            outcome.candidates >= 20,
            "the clean sweep must be over a real denominator, got {}",
            outcome.candidates
        );
    }

    /// Every guard must still be earning its place on this corpus. A guard
    /// that suppresses nothing here is either dead or has lost its test.
    #[test]
    fn the_correct_corpus_exercises_the_guards() {
        use crate::unused_local_binding::domain::Suppression::*;
        let outcome = run(CORRECT);
        for expected in [
            DeclaredIgnorable,
            ConventionallyUnused,
            LooksDynamic,
            DeclaredSpecial,
            OpaqueScope,
            UnexplainedOccurrence,
        ] {
            assert!(
                outcome
                    .suppressed
                    .iter()
                    .any(|(_, reason)| *reason == expected),
                "no candidate exercised {expected:?}; suppressed = {:?}",
                outcome.suppressed
            );
        }
    }

    /// The dangerous twin fires once per planted defect, and only there.
    #[test]
    fn the_dangerous_twin_reports_each_defect_exactly_once() {
        let outcome = run(DANGEROUS);
        let mut names: Vec<&str> = outcome
            .findings
            .iter()
            .map(|finding| finding.name.as_str())
            .collect();
        names.sort_unstable();
        assert_eq!(
            names,
            vec!["dead-four", "dead-one", "dead-three", "dead-two"],
            "suppressed = {:?}",
            outcome.suppressed
        );
    }
}
