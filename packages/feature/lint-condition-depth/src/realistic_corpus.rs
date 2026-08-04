//! A false-positive corpus for both rules: realistic *correct* Common Lisp that
//! touches every head they anchor on, linted through the real engine.
//!
//! Why this exists as well as each rule's own negative tests: a rule's own tests
//! are written by whoever wrote the rule, from the model that produced it, so
//! they encode the same blind spots. Running the *dispatch* over a file of
//! ordinary code catches what a per-rule test cannot — one rule firing on
//! another rule's recommended repair, and a rule firing on a form some other
//! rule in the suite already explains better.
//!
//! # The denominator is asserted, not assumed
//!
//! [`corpus_exercises_every_head`] pins the number of nodes each rule was
//! actually handed. **A zero-finding sweep over zero candidates is a
//! false-clean**, and it is the failure mode this file exists to make
//! impossible: without the denominator, deleting a head from a `HeadFilter`
//! would leave every test here green.
//!
//! # The dangerous twin
//!
//! [`DANGEROUS`] is the same program written wrongly, and it must fire each rule
//! **exactly once**. A corpus that only proves silence cannot distinguish a
//! careful rule from a broken one.
//!
//! Add to it rather than replace it: an entry in [`CORRECT`] is a claim that a
//! shape is correct code, and removing one is a decision to start reporting that
//! shape.

use std::path::Path;

use paredit_core_lint_engine::engine::{PassOptions, build_head_index, collect_lint_pass};
use paredit_core_lint_engine::policy::RuleSelection;
use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;

/// Both rules, wired exactly as a registry would wire them — so this exercises
/// `HeadFilter::Heads` dispatch, not a hand-rolled walk.
static ENTRIES: [RuleEntry; 2] = [
    RuleEntry::new(
        &crate::condition_type_datum_with_string_initarg::META,
        &crate::condition_type_datum_with_string_initarg::RULE,
    ),
    RuleEntry::new(
        &crate::unwind_protect_cleanup_signals::META,
        &crate::unwind_protect_cleanup_signals::RULE,
    ),
];

/// Realistic, **correct** Common Lisp condition handling.
///
/// Every line here is a claim that the shape is correct and must not be
/// reported. In particular it contains, deliberately:
///
/// - `define-condition` forms whose supertype list is `(error)` and
///   `(warning)`, which the head index hands to the datum rules as if they were
///   calls;
/// - the correct initarg spelling, and the correct `:format-control` spelling
///   that looks most like the defect;
/// - `error` with a genuine format-control datum and arguments;
/// - `cerror`, whose *first* argument is a string by design;
/// - `unwind-protect` cleanups that close, release and delete — all of which can
///   themselves signal a `file-error`, and none of which this rule reports;
/// - an `unwind-protect` cleanup that guards its own failure, which is the exact
///   repair `unwind-protect-cleanup-signals` recommends;
/// - a `handler-bind` handler that logs and **declines**, which is correct and
///   idiomatic;
/// - `handler-case` on broad types with non-inert bodies;
/// - a macro whose template contains both defects, as quoted data.
const CORRECT: &str = r#"
;;;; A small store, written the way the condition system intends.

(define-condition store-error (error)
  ((key :initarg :key :reader store-error-key))
  (:report (lambda (c s) (format s "no entry for ~S" (store-error-key c)))))

(define-condition store-stale (warning)
  ((age :initarg :age :reader store-stale-age))
  (:report (lambda (c s) (format s "entry is ~Ds old" (store-stale-age c)))))

(define-condition store-corrupt (store-error) ())

(defun fetch (store key)
  (multiple-value-bind (value found) (gethash key store)
    (unless found
      (error 'store-error :key key))
    value))

(defun fetch-checked (store key)
  (let ((value (fetch store key)))
    (when (stale-p value)
      (warn 'store-stale :age (entry-age value)))
    value))

(defun parse-entry (text)
  (when (zerop (length text))
    (error "empty entry: ~A" text))
  (handler-case (read-from-string text)
    (end-of-file (c) (error 'store-corrupt :key (princ-to-string c)))
    (reader-error () nil)))

(defun reconcile (a b)
  (cerror "Use the first entry." "entries disagree: ~S vs ~S" a b)
  a)

(defun with-entry (store key thunk)
  (let ((handle (acquire store key)))
    (unwind-protect (funcall thunk handle)
      (release handle))))

(defun copy-entry (source target)
  (let ((in (open source)) (out (open target :direction :output)))
    (unwind-protect (transfer in out)
      (close in)
      (close out))))

(defun with-scratch (path thunk)
  (unwind-protect (funcall thunk path)
    (delete-file path)))

(defun with-checked-cleanup (stream thunk)
  ;; The repair `unwind-protect-cleanup-signals` recommends: the cleanup's own
  ;; failure is handled rather than allowed to replace the original condition.
  (unwind-protect (funcall thunk stream)
    (handler-case (finish-output stream)
      (error (c) (log-secondary-failure c)))))

(defun retry-loop (store key)
  ;; A handler that logs and declines. Declining is a feature, not a defect.
  (handler-bind ((store-error (lambda (c) (log-warning "retrying: ~A" c))))
    (restart-case (fetch store key)
      (use-default () :report "Use the default entry." *default-entry*)
      (retry () :report "Try the fetch again." (fetch store key)))))

(defun guarded (store key)
  (handler-case (fetch store key)
    (store-error (c) (log-error c) nil)
    (error (c) (log-error c) :failed)))

(defun signal-progress (n)
  (signal 'store-progress :done n)
  (make-condition 'store-error :key n))

(defun report-both (a)
  (error 'simple-error
         :format-control "cannot reconcile ~S"
         :format-arguments (list a)))

(defmacro with-store ((var name) &body body)
  ;; Quoted template. Both defects appear here as *data* and must stay silent.
  `(let ((,var (open-store ,name)))
     (unwind-protect (progn ,@body)
       (error "template cleanup"))
     (error 'store-error "this is a template")))
"#;

/// The same program written wrongly. Each rule must fire **exactly once**.
const DANGEROUS: &str = r#"
(define-condition store-error (error)
  ((key :initarg :key :reader store-error-key))
  (:report "store error"))

(defun fetch (store key)
  ;; `"no entry for ~S"` lands in an initarg-name position: an odd-length
  ;; initializer list, so `store-error` is never signalled at all.
  (unless (gethash key store)
    (error 'store-error "no entry for ~S" key))
  (gethash key store))

(defun copy-entry (source target)
  (let ((in (open source)))
    ;; The cleanup replaces whatever condition was unwinding.
    (unwind-protect (transfer in target)
      (unless (complete-p target) (error "transfer incomplete")))))
"#;

/// Every finding of one source, as `rule: text` lines.
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

/// How many nodes each rule was handed, via the engine's own dispatch.
fn candidates(source: &str) -> Vec<(&'static str, u64)> {
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
    .expect("measured lint pass");
    outcome
        .timings
        .expect("measure: true produces timings")
        .entries()
        .map(|(position, _, invocations)| {
            (
                catalog.entries()[position].meta().name().as_str(),
                invocations,
            )
        })
        .collect()
}

fn candidates_for(source: &str, rule: &str) -> u64 {
    candidates(source)
        .into_iter()
        .find(|(name, _)| *name == rule)
        .map(|(_, invocations)| invocations)
        .expect("the rule is in the catalogue")
}

/// The denominator. Without this, deleting a head from a `HeadFilter` would
/// leave every other test in this file green.
#[test]
fn corpus_exercises_every_head() {
    let string_initarg = candidates_for(CORRECT, "condition-type-datum-with-string-initarg");
    let cleanup = candidates_for(CORRECT, "unwind-protect-cleanup-signals");

    // `error`/`cerror`/`signal`/`warn`/`make-condition` calls, plus the three
    // `define-condition` supertype lists `(error)`, `(warning)` and the one
    // inside the macro template.
    assert!(
        string_initarg >= 12,
        "the corpus must hand the datum rule real candidates, got {string_initarg}"
    );
    assert!(
        cleanup >= 5,
        "the corpus must hand the cleanup rule real unwind-protect forms, got {cleanup}"
    );
}

/// The whole point: correct code draws nothing.
#[test]
fn correct_code_yields_no_findings() {
    let found = findings(CORRECT);
    assert!(
        found.is_empty(),
        "correct Common Lisp must draw no findings, got:\n{}",
        found.join("\n")
    );
}

/// A corpus that only proves silence cannot tell a careful rule from a broken
/// one.
#[test]
fn the_dangerous_twin_fires_each_rule_exactly_once() {
    let found = findings(DANGEROUS);
    assert_eq!(
        found.len(),
        2,
        "expected exactly one finding per rule, got:\n{}",
        found.join("\n")
    );
    for rule in [
        "condition-type-datum-with-string-initarg",
        "unwind-protect-cleanup-signals",
    ] {
        assert_eq!(
            found.iter().filter(|line| line.contains(rule)).count(),
            1,
            "{rule} must fire exactly once, got:\n{}",
            found.join("\n")
        );
    }
}

/// The twin must exercise the same heads as the corpus, or "fires once" would
/// be a statement about a file the rules barely see.
#[test]
fn the_dangerous_twin_is_dispatched_like_the_corpus() {
    assert!(candidates_for(DANGEROUS, "condition-type-datum-with-string-initarg") >= 2);
    assert!(candidates_for(DANGEROUS, "unwind-protect-cleanup-signals") >= 1);
}
