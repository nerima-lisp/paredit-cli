//! A false-positive corpus for the six injection-shaped rules: correct Common
//! Lisp that touches every head they anchor on, linted through the real engine.
//!
//! Why this exists as well as each rule's own negative tests: a rule's own tests
//! are written by whoever wrote the rule, from the model that produced it, so
//! they encode the same blind spots. Running the *dispatch* over a file of
//! ordinary code catches what a per-rule test cannot — one rule firing on
//! another rule's recommended fix, and a rule firing on a form some *other*
//! rule in the suite already explains better.
//!
//! It has already earned its place. Two of the entries below were live false
//! positives found by this file and fixed:
//!
//! - `(with-open-file (s (format nil "/tmp/~a-~a" prefix (gensym)) …))` — the
//!   randomized scratch name that
//!   `insecure-temp-file-fixed-name-shared-directory` exists to recommend — was
//!   reported by `path-traversal-via-concatenated-filename`.
//! - `(format "~a~%" x)` — a `format` that forgot its destination, which
//!   `format-missing-destination` already names — was reported by
//!   `format-tilde-slash-unvalidated-function-designator` as an opaque control
//!   string.
//!
//! Add to it rather than replace it: an entry here is a claim that a shape is
//! correct code, and removing one is a decision to start reporting that shape.

use paredit_core_lint_engine::engine::{PassOptions, build_head_index, collect_lint_pass};
use paredit_core_lint_engine::policy::RuleSelection;
use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;
use std::path::Path;

/// The six rules, wired exactly as the root registry wires them — so this
/// exercises `HeadFilter::Heads` dispatch, not a hand-rolled walk.
static ENTRIES: [RuleEntry; 6] = [
    RuleEntry::new(
        &crate::format_tilde_slash_unvalidated_function_designator::META,
        &crate::format_tilde_slash_unvalidated_function_designator::RULE,
    ),
    RuleEntry::new(
        &crate::insecure_temp_file_fixed_name_shared_directory::META,
        &crate::insecure_temp_file_fixed_name_shared_directory::RULE,
    ),
    RuleEntry::new(
        &crate::read_eval_star_rebound_to_t::META,
        &crate::read_eval_star_rebound_to_t::RULE,
    ),
    RuleEntry::new(
        &crate::path_traversal_via_concatenated_filename::META,
        &crate::path_traversal_via_concatenated_filename::RULE,
    ),
    RuleEntry::new(
        &crate::sql_query_string_built_via_format::META,
        &crate::sql_query_string_built_via_format::RULE,
    ),
    RuleEntry::new(
        &crate::world_writable_file_mode_in_open_call::META,
        &crate::world_writable_file_mode_in_open_call::RULE,
    ),
];

fn findings(source: &str) -> Vec<String> {
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
        PassOptions::default(),
    )
    .expect("lint pass");
    outcome
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
                .to_owned();
            format!("line {line}: [{}] {text}", finding.rule)
        })
        .collect()
}

/// Correct Common Lisp exercising `format`, `open`, `with-open-file`,
/// `probe-file`, `load`, `chmod`, `setf`, `let`, `let*`, `query` and `execute`.
const CORRECT: &str = r#"
(defpackage #:app.report (:use #:cl) (:export #:render #:store))
(in-package #:app.report)

(defparameter *data-directory* #p"/var/lib/app/")
(defparameter *usage* "usage: app [options]~%")
(defconstant +row-format+ "~&~a~30t~a~%")

;;; format: literal controls, program-owned controls, forwarded controls.
(defun usage (stream)
  (format stream *usage*)
  (format t "~a" +row-format+)
  (write-string *usage* stream))

(defun render-row (stream label value)
  (format stream +row-format+ label value)
  (format stream "~&~a: ~s~%" label value)
  (format nil "~{~a~^, ~}" (list label value))
  (format nil "~10,'0d" value)
  (format t "~/app.report:print-row/" value))

(defun log-line (stream control &rest arguments)
  (format stream "~&[~a] " (get-universal-time))
  (apply #'format stream control arguments))

(defun choose-control (verbose)
  (format t (if verbose "~&~a~%" "~a")))

;;; A format that forgot its destination: malformed, and
;;; `format-missing-destination` is the rule that says so.
(defun missing-destination (x) (format "~a~%" x))

;;; pathnames: merge-pathnames, truename, pathname-name, literal joins.
(defun report-path (name)
  (merge-pathnames (make-pathname :name name :type "txt") *data-directory*))

(defun safe-report-path (name)
  (merge-pathnames (pathname-name name) *data-directory*))

(defun joined-path (directory name)
  (format nil "~a/~a" directory name))

(defun joined-by-concatenate (directory name)
  (concatenate 'string directory "/" name))

(defun canonical (name)
  (open (truename (concatenate 'string "data/" name))))

(defun narrowed (name)
  (open (concatenate 'string "data/" (pathname-name name))))

(defun fixed-index ()
  (open (concatenate 'string "data/" "index.txt")))

(defun load-module (name)
  (load (merge-pathnames (pathname-name name) #p"modules/")))

(defun probe-report (name)
  (probe-file (merge-pathnames name *data-directory*)))

;;; temporary files: randomized names, and the library idioms.
(defun scratch-file (body)
  (uiop:with-temporary-file (:stream stream :pathname path)
    (write-string body stream)
    (finish-output stream)
    path))

(defun randomized-temp (prefix)
  (with-open-file (stream (format nil "/tmp/~a-~a" prefix (gensym))
                          :direction :output
                          :if-exists :error)
    stream))

(defun randomized-cache (prefix)
  (open (concatenate 'string "cache/" prefix "-" (random 1000000))
        :direction :output))

(defun lock-file ()
  (open (uiop:tmpize-pathname (report-path "lock")) :direction :output))

(defun read-fixed-temp ()
  ;; A *read* of a fixed temporary path: not this suite's finding.
  (with-open-file (stream "/tmp/app.state") (read-line stream)))

;;; writes outside a shared directory.
(defun write-report (name rows)
  (with-open-file (stream (report-path name)
                          :direction :output
                          :if-exists :supersede)
    (dolist (row rows) (render-row stream (car row) (cdr row)))))

(defun append-log (line)
  (with-open-file (stream "/var/log/app.log" :direction :output :if-exists :append)
    (write-line line stream)))

;;; permissions without the world-write bit, and the sticky shared directory.
(defun harden (path)
  (sb-posix:chmod path #o600)
  (sb-posix:chmod (report-path "public") #o644)
  (sb-posix:chmod (report-path "shared") #o664)
  (sb-posix:chmod (report-path "bin") #o755)
  (sb-posix:chmod "/var/spool/app" #o1777)
  (sb-posix:chmod path *default-mode*)
  (sb-posix:chmod path 438))

;;; *read-eval*: the guard, and specials that are not it.
(defun read-config (name)
  (with-open-file (stream (merge-pathnames name *data-directory*))
    (let ((*read-eval* nil))
      (read stream))))

(defun print-hex (value)
  (let ((*print-base* 16) (*print-pretty* t))
    (princ value)))

(defun restore-read-eval (saved)
  (setf *read-eval* saved))

(defun bump-counter ()
  (setf *report-count* (1+ *report-count*)))

;;; SQL: literal statements, bound parameters, and English prose.
(defun find-user (connection name)
  (query connection "SELECT id, name FROM users WHERE name = $1" name))

(defun count-rows (connection)
  (execute connection "SELECT count(*) FROM users"))

(defun constant-id-query (connection)
  (query connection (format nil "SELECT id FROM users WHERE id = ~a" 42)))

(defun describe-result (n)
  (execute (format nil "~a rows selected" n)))

(defun insert-disc-prompt (n)
  (execute (format nil "Insert the disc labelled ~a" n)))

(defun update-notice (version)
  (execute (format nil "update available: ~a" version)))

;;; quoted data that spells out dangerous forms.
(defvar *templates* '((:open . (open "/tmp/x" :if-exists :supersede))
                      (:chmod . (chmod "/tmp/x" #o777))
                      (:arm . (setf *read-eval* t))
                      (:sql . (query c (format nil "SELECT a FROM t WHERE b = ~a" v)))))

(defmacro with-report ((stream name) &body body)
  `(with-open-file (,stream (report-path ,name) :direction :output)
     ,@body))

(defmacro define-sql-accessor (name statement)
  `(defun ,name (connection key)
     (query connection ,statement key)))

;;; text that only looks like a call.
(defun documentation-line ()
  (write-string "(chmod \"/tmp/f\" #o777) is what not to write"))
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correct_code_produces_no_findings() {
        let found = findings(CORRECT);
        assert!(found.is_empty(), "false positives:\n{}", found.join("\n"));
    }

    /// The corpus must be able to fail: a harness that reports "no findings" on
    /// code that *should* report is worth nothing. This is the same file's
    /// dangerous twin, and every one of the six rules must fire on it.
    #[test]
    fn the_corpus_harness_can_detect_findings() {
        const DANGEROUS: &str = r#"
(defun a (stream message) (format stream message))
(defun b (name) (with-open-file (s "/tmp/report.out" :direction :output) (write-line name s)))
(defun c () (setf *read-eval* t))
(defun d (name) (open (concatenate 'string "data/" name)))
(defun e (conn name) (query conn (format nil "SELECT id FROM users WHERE name = '~a'" name)))
(defun f (path) (chmod path #o666))
"#;
        let found = findings(DANGEROUS);
        assert_eq!(found.len(), 6, "{found:#?}");
        for rule in [
            "format-tilde-slash-unvalidated-function-designator",
            "insecure-temp-file-fixed-name-shared-directory",
            "read-eval-star-rebound-to-t",
            "path-traversal-via-concatenated-filename",
            "sql-query-string-built-via-format",
            "world-writable-file-mode-in-open-call",
        ] {
            assert!(
                found.iter().any(|item| item.contains(rule)),
                "{rule} did not fire: {found:#?}"
            );
        }
    }
}
