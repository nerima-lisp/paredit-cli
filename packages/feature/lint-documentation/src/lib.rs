#![doc = include_str!("../README.md")]

pub mod docstring_example_stale_arity;
pub mod docstring_summary_line_too_long;
pub mod missing_package_docstring;
pub mod support;
pub mod todo_fixme_no_attribution;

/// The realistic-correct code every rule here is swept over, embedded because
/// this repository's treefmt would otherwise reformat it. Tests only.
#[cfg(test)]
mod corpus;

/// The false-positive sweep, over the embedded corpus and over this
/// repository's own Lisp fixtures. Tests only.
#[cfg(test)]
mod fixture_sweep;

/// The measured per-rule cost, and the two properties CI's `bench-compare`
/// gate is really about. Tests only.
#[cfg(test)]
mod cost_probe;

// The root's REGISTRY names each rule's META and RULE across this crate
// boundary (section 4.2). No rule here owns a subcommand: a standalone
// `inspect <rule>` command is presentation wiring, and is not this package's.

/// Every rule here driven through the *engine*, rather than through its own
/// `examine`.
///
/// Three things a domain test structurally cannot see:
///
/// - **The head filter.** A rule that declares the wrong head compiles, passes
///   every domain test, and is simply never invoked by the CLI.
/// - **The quote guard.** A domain test hands `examine` whichever node it
///   picked; the dispatcher hands a rule *every* head-matched node, including
///   the ones inside `'(…)` and inside a macro's `` `(…) `` template. Without
///   each `check`'s `is_unevaluated_at` call, the three node-based rules here
///   fire on every quoted example in a macro's documentation.
/// - **The dialect scope.** The dispatcher skips a rule before walking
///   anything, so a wrong scope is invisible until a file in the wrong dialect
///   is linted.
///
/// The last test in this module is the one that matters most. A reviewer runs
/// realistic, correct code first, and a documentation rule that nags on it gets
/// the whole category switched off; `a_realistic_correct_file_produces_no_findings`
/// is that check, made permanent. It is paired with
/// `the_sweep_harness_can_still_detect_findings`, because "no findings" from a
/// broken harness proves nothing at all.
#[cfg(test)]
mod engine_pass_tests {
    use std::path::Path;

    use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
    use paredit_core_lint_engine::policy::RuleSelection;
    use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    static ENTRIES: [RuleEntry; 4] = [
        RuleEntry::new(
            &crate::docstring_example_stale_arity::rule::META,
            &crate::docstring_example_stale_arity::rule::RULE,
        ),
        RuleEntry::new(
            &crate::docstring_summary_line_too_long::rule::META,
            &crate::docstring_summary_line_too_long::rule::RULE,
        ),
        RuleEntry::new(
            &crate::missing_package_docstring::rule::META,
            &crate::missing_package_docstring::rule::RULE,
        ),
        RuleEntry::new(
            &crate::todo_fixme_no_attribution::rule::META,
            &crate::todo_fixme_no_attribution::rule::RULE,
        ),
    ];

    /// The rule names that fire on `source`, sorted so the assertions do not
    /// depend on registration order.
    fn fired(source: &str, dialect: Dialect) -> Vec<&'static str> {
        let catalog = RuleCatalog::new(&ENTRIES);
        let index = build_head_index(catalog);
        let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
        let mut names: Vec<&'static str> = collect_lint_outcomes(
            catalog,
            &index,
            Path::new("t.lisp"),
            dialect,
            &tree,
            source,
            RuleSelection::All,
        )
        .expect("lint pass")
        .into_iter()
        .map(|outcome| outcome.into_parts().0.rule)
        .collect();
        names.sort_unstable();
        names
    }

    // -- each rule reaches the engine ---------------------------------------

    #[test]
    fn every_rule_fires_through_the_real_dispatch() {
        assert_eq!(
            fired(
                "(defun scale (x factor) \"Example: (scale 3)\" (* x factor))",
                Dialect::CommonLisp
            ),
            vec!["docstring-example-stale-arity"]
        );
        // A 200-character summary line, and an example-free docstring so this
        // isolates the width rule.
        let wide = "word ".repeat(60);
        assert_eq!(
            fired(
                &format!("(defun f (x) \"{wide}\" (+ x 1))"),
                Dialect::CommonLisp
            ),
            vec!["docstring-summary-line-too-long"]
        );
        assert_eq!(
            fired("(defpackage :app (:use :cl))", Dialect::CommonLisp),
            vec!["missing-package-docstring"]
        );
        assert_eq!(
            fired(";; TODO: handle the empty case\n(f)\n", Dialect::CommonLisp),
            vec!["todo-fixme-no-attribution"]
        );
    }

    /// The four rules are independent: a file that trips all of them trips all
    /// of them, and no rule swallows another's node.
    ///
    /// The `TODO` sits *below* the declaration on purpose. Put above it, that
    /// same comment is prose before a `defpackage` and therefore documents the
    /// package — which is `missing-package-docstring`'s comment guard doing its
    /// job, and which cost this test a first draft.
    #[test]
    fn one_file_can_trip_every_rule_at_once() {
        let wide = "word ".repeat(60);
        let source = format!(
            "(defpackage :app (:use :cl))\n\
             (in-package :app)\n\
             ;; TODO: split this file\n\
             (defun scale (x factor) \"Example: (scale 3)\" (* x factor))\n\
             (defun g (y) \"{wide}\" (+ y 1))\n"
        );
        assert_eq!(
            fired(&source, Dialect::CommonLisp),
            vec![
                "docstring-example-stale-arity",
                "docstring-summary-line-too-long",
                "missing-package-docstring",
                "todo-fixme-no-attribution",
            ]
        );
    }

    // -- the guards the domain tests cannot exercise -------------------------

    /// The dispatcher hands every head-matched node to its rule, quoted or not.
    /// `todo-fixme-no-attribution` is the exception at both ends: its subject
    /// sits *beside* the tree, so quoting a neighbouring form says nothing
    /// about it — which is why these sources carry no marker.
    #[test]
    fn no_node_based_rule_fires_inside_the_five_quote_shapes() {
        let wide = "word ".repeat(60);
        for source in [
            "'(defun scale (x factor) \"Example: (scale 3)\" x)".to_owned(),
            "(quote (defpackage :app (:use :cl)))".to_owned(),
            "'(a ,(defpackage :app (:use :cl)))".to_owned(),
            format!("`(defun f (x) \"{wide}\" (+ x 1))"),
            format!("(defmacro def-thing (n) `(defun ,n (x) \"{wide}\" (+ x 1)))"),
        ] {
            assert_eq!(
                fired(&source, Dialect::CommonLisp),
                Vec::<&str>::new(),
                "{source} is quoted data"
            );
        }
    }

    /// The one shape that is code again.
    #[test]
    fn an_unquoted_form_inside_a_quasiquote_still_fires() {
        assert_eq!(
            fired("`(a ,(defpackage :app (:use :cl)))", Dialect::CommonLisp),
            vec!["missing-package-docstring"]
        );
    }

    /// `RuleDialectScope`: the three docstring rules encode Common Lisp's own
    /// grammar and are skipped before anything is walked. The comment rule is
    /// declared for every dialect, because `;` means the same thing in all of
    /// them.
    #[test]
    fn only_the_comment_rule_runs_outside_common_lisp() {
        for dialect in [Dialect::Clojure, Dialect::Scheme, Dialect::Fennel] {
            assert_eq!(
                fired("(defpackage :app (:use :cl))", dialect),
                Vec::<&str>::new(),
                "a docstring rule ran in {dialect:?}"
            );
            assert_eq!(
                fired(";; TODO: later\n(f)\n", dialect),
                vec!["todo-fixme-no-attribution"],
                "the comment rule did not run in {dialect:?}"
            );
        }
    }

    /// `HeadFilter::Heads`: an ordinary definition is never handed to the three
    /// head-filtered rules, which is what keeps the zero-finding benchmarks
    /// cheap. `todo-fixme-no-attribution` is `WholeTree` and *does* see this
    /// file, and says nothing about it, which is the other half of the same
    /// assertion.
    #[test]
    fn an_irrelevant_file_trips_nothing() {
        for source in [
            "(defun add (x y) (+ x y))\n(defmethod area ((s square)) 1)\n",
            "(let ((x 1)) (loop for i from 1 to x collect i))\n",
            "(defclass point () ((x :initarg :x) (y :initarg :y)))\n",
            "",
        ] {
            assert_eq!(
                fired(source, Dialect::CommonLisp),
                Vec::<&str>::new(),
                "fired on an irrelevant file: {source}"
            );
        }
    }

    // -- the false-positive sweep -------------------------------------------

    /// Realistic, correct Common Lisp — the file a reviewer writes to see
    /// whether a new rule family is usable. Every shape here is one of the four
    /// rules' near misses, written the way a careful author writes it.
    ///
    /// This is the sweep, made permanent. It is only meaningful together with
    /// [`the_sweep_harness_can_still_detect_findings`] below: a harness that
    /// silently reported nothing would pass this test and prove nothing.
    const REALISTIC_CORRECT: &str = "\
;;;; app.lisp — the application's public interface.
;;;;
;;;; Everything a caller needs is exported from this package.

(defpackage #:app
  (:use #:cl)
  (:export #:scale #:retry #:*timeout*)
  (:documentation \"The application's public interface.\"))

(in-package #:app)

(defparameter *timeout* 30
  \"Seconds to wait for the server before giving up.\")

(defvar *cache* nil
  \"Memoized results, keyed by request id.\")

(defconstant +limit+ 100 \"The largest batch this accepts.\")

(defun scale (x factor)
  \"Return X scaled by FACTOR.

Example: (scale 3 2) => 6\"
  (* x factor))

(defun retry (n thunk)
  \"Attempt THUNK up to N times.

Returns the first successful value, re-signalling the last condition if every
attempt fails. Example: (retry 3 (lambda () (fetch)))\"
  (funcall thunk))

(defun total (&rest numbers)
  \"Sum NUMBERS.

Example: (total 1 2 3 4 5) => 15\"
  (apply #'+ numbers))

(defun render (object &key stream pretty)
  \"Write OBJECT to STREAM.

Example: (render x :stream s :pretty t)\"
  (declare (ignore pretty))
  (print object stream))

(defmacro with-timeout ((seconds) &body body)
  \"Run BODY with a SECONDS deadline.

Example: (with-timeout (5) (fetch) (parse))\"
  `(progn ,@body))

(defun greeting ()
  \"hello\")

(defmethod area ((s square))
  \"The area of S.\"
  (* (side s) (side s)))

;; TODO(ada): memoize this once #412 lands.
(defun expensive (x)
  \"Compute the thing for X.\"
  (identity x))

;; FIXME: see PROJ-88 — the fallback path is untested.
(defun fallback (x)
  \"Return X unchanged.\"
  x)

;; HACK: works around https://example.com/bugs/9 until the fix ships.
(defun workaround (x)
  \"Return X unchanged.\"
  x)

;; XXX: revisit after 2026-12-01.
(defun scheduled (x)
  \"Return X unchanged.\"
  x)
";

    #[test]
    fn a_realistic_correct_file_produces_no_findings() {
        let found = fired(REALISTIC_CORRECT, Dialect::CommonLisp);
        assert_eq!(
            found,
            Vec::<&str>::new(),
            "a documentation rule fired on correct code"
        );
    }

    /// The dangerous twin. Without this, `a_realistic_correct_file_produces_no_findings`
    /// would pass just as happily on a harness that had stopped working — which
    /// is exactly how a broken sweep reports all-green.
    #[test]
    fn the_sweep_harness_can_still_detect_findings() {
        // The same file, with each rule's guard broken one at a time.
        let stale_example = REALISTIC_CORRECT.replace("(scale 3 2) => 6", "(scale 3) => 6");
        assert_eq!(
            fired(&stale_example, Dialect::CommonLisp),
            vec!["docstring-example-stale-arity"]
        );

        let bare_marker = REALISTIC_CORRECT.replace(
            "TODO(ada): memoize this once #412 lands.",
            "TODO: memoize this.",
        );
        assert_eq!(
            fired(&bare_marker, Dialect::CommonLisp),
            vec!["todo-fixme-no-attribution"]
        );

        let undocumented_package = REALISTIC_CORRECT.replace(
            "  (:documentation \"The application's public interface.\"))",
            "  )",
        );
        // The file header comment still documents it, so removing the option
        // alone is not enough — which is itself the guard being exercised.
        assert_eq!(
            fired(&undocumented_package, Dialect::CommonLisp),
            Vec::<&str>::new()
        );
        let header_stripped = undocumented_package
            .lines()
            .filter(|line| !line.starts_with(";;;;"))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            fired(&header_stripped, Dialect::CommonLisp),
            vec!["missing-package-docstring"]
        );

        let wide = "word ".repeat(60);
        let long_summary = REALISTIC_CORRECT.replace("Return X scaled by FACTOR.", wide.trim_end());
        assert_eq!(
            fired(&long_summary, Dialect::CommonLisp),
            vec!["docstring-summary-line-too-long"]
        );
    }

    /// Realistic, correct code in the other dialects the comment rule is
    /// declared for. The docstring rules are out of scope there, so this is
    /// entirely about `todo-fixme-no-attribution` not nagging.
    #[test]
    fn realistic_correct_code_in_other_dialects_produces_no_findings() {
        let clojure = "(ns app.core\n  \"The application's public interface.\"\n  \
             (:require [clojure.string :as str]))\n\n\
             ;; TODO(ada): drop this once #412 lands.\n\
             (defn scale\n  \"Return x scaled by factor.\"\n  [x factor]\n  (* x factor))\n";
        assert_eq!(fired(clojure, Dialect::Clojure), Vec::<&str>::new());

        let elisp = ";;; app.el --- The application -*- lexical-binding: t -*-\n\n\
             ;;; Commentary:\n\
             ;; Everything a caller needs.\n\n\
             ;;; Code:\n\n\
             (defun app-scale (x factor)\n  \"Return X scaled by FACTOR.\"\n  (* x factor))\n\n\
             (provide 'app)\n;;; app.el ends here\n";
        assert_eq!(fired(elisp, Dialect::EmacsLisp), Vec::<&str>::new());
    }
}
