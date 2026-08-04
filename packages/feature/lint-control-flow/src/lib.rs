#![doc = include_str!("../README.md")]

pub mod binds_constant;
pub mod block_name_shadows_outer_block;
pub mod dotimes_dolist_index_var_mutated;
pub mod eval_when_situation;
pub mod explicit_nil_return;
pub mod go_to_undefined_tag;
pub mod handler_case_no_clauses;
pub mod malformed_iteration_spec;
pub mod multiple_value_bind_all_ignored;
pub mod nested_progn;
pub mod prog2_to_progn;
pub mod redundant_body_progn;
pub mod redundant_prog1;
pub mod redundant_progn;
pub mod return_from_unmatched_block;
pub mod return_outside_implicit_nil_block;
pub mod self_recursive_tail_call;
pub mod support;

#[cfg(test)]
mod quote_guard_tests;
pub mod tagbody_unreachable_tag;
pub mod unwind_protect_no_cleanup;

// The root's REGISTRY names each rule's META and RULE across this crate
// boundary (section 4.2), and each slice's cli owns its own subcommand.

/// The seven non-local-exit rules driven through the *engine*, rather than
/// through their own `build_*_report`.
///
/// The two entry points do not share their reachability. A report walks every
/// node itself; a head-filtered rule is only ever handed the nodes its
/// `HeadFilter::Heads` list names, and a wrong entry there passes every
/// `examine_*` test in this crate while being unreachable from the CLI. This
/// module is what asks the dispatcher.
///
/// Running the real pass also covers the two declarations a domain test cannot
/// see: each rule's head filter and its `RuleDialectScope`.
#[cfg(test)]
mod engine_pass_tests {
    use std::path::Path;

    use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
    use paredit_core_lint_engine::policy::RuleSelection;
    use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    /// Only the seven rules this batch added. A local catalogue rather than
    /// the root's `REGISTRY`: this crate cannot name that (it is the
    /// composition root's, and naming it would be a cycle), and a catalogue of
    /// exactly these seven makes "which rule fired" an equality assertion
    /// rather than a filter.
    static ENTRIES: [RuleEntry; 7] = [
        RuleEntry::new(
            &crate::block_name_shadows_outer_block::rule::META,
            &crate::block_name_shadows_outer_block::rule::RULE,
        ),
        RuleEntry::new(
            &crate::dotimes_dolist_index_var_mutated::rule::META,
            &crate::dotimes_dolist_index_var_mutated::rule::RULE,
        ),
        RuleEntry::new(
            &crate::go_to_undefined_tag::rule::META,
            &crate::go_to_undefined_tag::rule::RULE,
        ),
        RuleEntry::new(
            &crate::multiple_value_bind_all_ignored::rule::META,
            &crate::multiple_value_bind_all_ignored::rule::RULE,
        ),
        RuleEntry::new(
            &crate::return_from_unmatched_block::rule::META,
            &crate::return_from_unmatched_block::rule::RULE,
        ),
        RuleEntry::new(
            &crate::return_outside_implicit_nil_block::rule::META,
            &crate::return_outside_implicit_nil_block::rule::RULE,
        ),
        RuleEntry::new(
            &crate::tagbody_unreachable_tag::rule::META,
            &crate::tagbody_unreachable_tag::rule::RULE,
        ),
    ];

    /// The rule names that fire on `source`, sorted so the assertions do not
    /// depend on registration order.
    pub(super) fn fired(source: &str, dialect: Dialect) -> Vec<&'static str> {
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
                "(block scan (block scan (return-from scan 1)))",
                Dialect::CommonLisp
            ),
            vec!["block-name-shadows-outer-block"]
        );
        assert_eq!(
            fired("(dotimes (i 10) (setq i 20))", Dialect::CommonLisp),
            vec!["dotimes-dolist-index-var-mutated"]
        );
        assert_eq!(
            fired(
                "(tagbody start (go finish) (foo start))",
                Dialect::CommonLisp
            ),
            vec!["go-to-undefined-tag"]
        );
        assert_eq!(
            fired(
                "(multiple-value-bind (q r) (floor 7 2) (print 1))",
                Dialect::CommonLisp
            ),
            vec!["multiple-value-bind-all-ignored"]
        );
        assert_eq!(
            fired("(defun f () (return-from g 1))", Dialect::CommonLisp),
            vec!["return-from-unmatched-block"]
        );
        assert_eq!(
            fired("(defun f () (return 1))", Dialect::CommonLisp),
            vec!["return-outside-implicit-nil-block"]
        );
        assert_eq!(
            fired("(tagbody start (foo))", Dialect::CommonLisp),
            vec!["tagbody-unreachable-tag"]
        );
    }

    /// A file with none of the seven heads must trip nothing at all. This is
    /// the shape the `clean/forms/*` benchmarks lint.
    #[test]
    fn a_file_with_none_of_these_heads_trips_nothing() {
        assert_eq!(
            fired(
                "(defun add (a b)\n  (let ((sum (+ a b)))\n    (format t \"~a\" sum)\n    sum))\n",
                Dialect::CommonLisp
            ),
            Vec::<&str>::new()
        );
    }

    // -- the guard a domain test cannot exercise -----------------------------

    /// The dispatcher hands a rule every head-matched node, quoted or not.
    /// Without each rule's own quote check, every one of these fires.
    #[test]
    fn no_rule_fires_on_hard_quoted_data() {
        for source in [
            "'(block scan (block scan (return-from scan 1)))",
            "'(dotimes (i 10) (setq i 20))",
            "'(tagbody start (go finish) (foo start))",
            "'(multiple-value-bind (q r) (floor 7 2) (print 1))",
            "'(defun f () (return-from g 1))",
            "'(defun f () (return 1))",
            "'(tagbody start (foo))",
        ] {
            assert_eq!(
                fired(source, Dialect::CommonLisp),
                Vec::<&str>::new(),
                "{source} is quoted data"
            );
        }
    }

    #[test]
    fn no_rule_fires_inside_a_long_hand_quote_form() {
        assert_eq!(
            fired("(quote (tagbody start (foo)))", Dialect::CommonLisp),
            Vec::<&str>::new()
        );
        assert_eq!(
            fired("(quote (defun f () (return 1)))", Dialect::CommonLisp),
            Vec::<&str>::new()
        );
    }

    /// A comma inside a hard quote is a literal comma, not an escape back to
    /// code — the shape a single depth counter reads wrongly.
    #[test]
    fn no_rule_fires_on_a_comma_inside_a_hard_quote() {
        assert_eq!(
            fired("'(a ,(tagbody start (foo)))", Dialect::CommonLisp),
            Vec::<&str>::new()
        );
        assert_eq!(
            fired("'(a ,(dotimes (i 10) (setq i 20)))", Dialect::CommonLisp),
            Vec::<&str>::new()
        );
    }

    #[test]
    fn no_rule_fires_inside_a_quasiquoted_macro_template() {
        assert_eq!(
            fired(
                "(defmacro m () `(dotimes (i 10) (setq i 20)))",
                Dialect::CommonLisp
            ),
            Vec::<&str>::new()
        );
    }

    /// An unquote escapes back to code, so this one is reached.
    #[test]
    fn a_rule_fires_on_an_unquoted_form_inside_a_quasiquote() {
        assert_eq!(
            fired(
                "(defmacro m () `(a ,(dotimes (i 10) (setq i 20))))",
                Dialect::CommonLisp
            ),
            vec!["dotimes-dolist-index-var-mutated"]
        );
    }

    // -- the dialect declaration --------------------------------------------

    /// Every one of these encodes CLHS operator semantics. `dotimes`,
    /// `dolist` and `block` are spelled the same in Emacs Lisp and mean
    /// something close enough to be tempting, which is exactly why the scope
    /// declaration has to be tested rather than assumed.
    #[test]
    fn no_rule_fires_on_another_dialect() {
        for dialect in [Dialect::Clojure, Dialect::EmacsLisp, Dialect::Scheme] {
            assert_eq!(
                fired("(dotimes (i 10) (setq i 20))", dialect),
                Vec::<&str>::new(),
                "{dialect:?}"
            );
        }
    }
}

/// The seven rules run over realistic, *correct* Common Lisp — the check that
/// no unit test can perform on itself.
///
/// Author-written unit tests encode the author's model of the language, so a
/// wrong model passes them all. This module runs the real engine over code
/// written to be correct rather than to exercise a predicate, and asserts a
/// finding count of zero. Its twin, `the_harness_can_still_detect_findings`,
/// runs the same harness over deliberately broken code: without it, "no
/// findings" would also be what a silently broken harness reports.
#[cfg(test)]
mod realistic_code_tests {
    use std::path::{Path, PathBuf};

    use super::engine_pass_tests::fired;
    use paredit_core_syntax::dialect::Dialect;

    /// Correct Common Lisp exercising every shape these seven rules look at:
    /// exits that resolve, tags that are jumped to, bindings that are read or
    /// declared ignored, and loop variables left alone.
    const CORRECT: &str = r#"
(defpackage #:inventory
  (:use #:common-lisp)
  (:export #:find-item #:restock))

(in-package #:inventory)

(defun find-item (items key)
  "Returns the first item whose key matches, or NIL."
  (dolist (item items nil)
    (when (eql (car item) key)
      (return-from find-item item))))

(defun first-even (numbers)
  (dolist (n numbers)
    (when (evenp n)
      (return n))))

(defun scan-until (numbers limit)
  (loop for n in numbers
        when (> n limit)
          do (return n)
        finally (return nil)))

(defun search-grid (grid target)
  (loop named outer
        for row across grid
        do (loop for cell across row
                 when (eql cell target)
                   do (return-from outer cell))))

(defun retry-connect (attempts)
  (let ((tries 0))
    (tagbody
     retry
       (incf tries)
       (when (and (not (connect)) (< tries attempts))
         (go retry))
       (unless (connected-p)
         (error "gave up after ~d tries" tries)))
    tries))

(defun classify (value)
  (multiple-value-bind (quotient remainder) (floor value 10)
    (if (zerop remainder)
        quotient
        (list quotient remainder))))

(defun present-p (table key)
  (multiple-value-bind (value found) (gethash key table)
    (declare (ignore value))
    found))

(defun total (items)
  (let ((sum 0))
    (dolist (item items sum)
      (incf sum (item-cost item)))))

(defun normalized-names (items)
  (let ((names '()))
    (dolist (item items (nreverse names))
      (let ((name (item-name item)))
        (push (string-downcase name) names)))))

(defun scale-all (values factor)
  (dotimes (i (length values) values)
    (setf (aref values i) (* factor (aref values i)))))

(defun restock (items)
  (block restocking
    (dolist (item items)
      (when (null item)
        (return-from restocking :aborted))
      (order-more item))
    :done))

(defmacro with-retries ((count) &body body)
  `(block retry-block
     (dotimes (attempt ,count)
       (declare (ignorable attempt))
       (let ((result (progn ,@body)))
         (when result
           (return-from retry-block result))))))

(defun report (stream items)
  (flet ((emit (item)
           (unless item
             (return-from emit nil))
           (format stream "~a~%" item)))
    (mapc #'emit items))
  (values))

(defun collect-pairs (alist)
  (let ((pairs '()))
    (dolist (entry alist (nreverse pairs))
      (multiple-value-bind (key value) (values (car entry) (cdr entry))
        (push (cons key value) pairs)))))

(defun step-through (items)
  (dolist (item items)
    (let ((item (normalize item)))
      (setq item (annotate item))
      (record item))))
"#;

    /// The same harness over code that is deliberately wrong, one shape per
    /// rule. Without this, a harness that silently reports nothing would make
    /// the sweep above pass while proving nothing.
    const BROKEN: &str = r#"
(defun stray-exit (items)
  (dolist (item items)
    (return-from no-such-block item)))

(defun stray-return ()
  (let ((x 1))
    (return x)))

(defun stray-jump ()
  (tagbody
     start
     (go finish)))

(defun dead-label ()
  (tagbody
     unused-label
     (compute)))

(defun ignored-values ()
  (multiple-value-bind (quotient remainder) (floor 7 2)
    (compute-something-else)))

(defun shadowed-block (items)
  (block scan
    (dolist (item items)
      (block scan
        (return-from scan item)))))

(defun mutated-index (n)
  (dotimes (i n)
    (setq i (* 2 i))))
"#;

    fn findings_in(source: &str) -> Vec<&'static str> {
        fired(source, Dialect::CommonLisp)
    }

    #[test]
    fn realistic_correct_common_lisp_produces_no_findings() {
        assert_eq!(
            findings_in(CORRECT),
            Vec::<&str>::new(),
            "a finding here is a false positive on code that is correct"
        );
    }

    /// The dangerous twin: proves the sweep above can see a finding at all.
    #[test]
    fn the_harness_can_still_detect_findings() {
        let mut fired = findings_in(BROKEN);
        fired.dedup();
        assert_eq!(
            fired,
            vec![
                "block-name-shadows-outer-block",
                "dotimes-dolist-index-var-mutated",
                "go-to-undefined-tag",
                "multiple-value-bind-all-ignored",
                "return-from-unmatched-block",
                "return-outside-implicit-nil-block",
                "tagbody-unreachable-tag",
            ]
        );
    }

    /// The repository's own Common Lisp, linted through the real engine.
    ///
    /// These files were written as parser, formatter and semantic-analysis
    /// fixtures, not as lint subjects, which is what makes them useful here:
    /// nothing in them was shaped to please these seven rules.
    ///
    /// `tests/fixtures/lint_golden/` is deliberately excluded and asserted
    /// about separately — those files exist to *trip* lint rules, so a finding
    /// there is the fixture working, not a false positive.
    #[test]
    fn the_repositorys_own_lisp_fixtures_produce_no_findings() {
        let mut scanned = 0;
        for path in repository_lisp_files(&[
            "tests/fixtures/corpus",
            "tests/fixtures/semantic_coverage_corpus",
            "packages/feature/migrate/recipes",
        ]) {
            let source = std::fs::read_to_string(&path).expect("read fixture");
            scanned += 1;
            assert_eq!(
                findings_in(&source),
                Vec::<&str>::new(),
                "{} earned a finding",
                path.display()
            );
        }
        assert!(
            scanned > 0,
            "the fixture sweep found no files to read; it proves nothing"
        );
    }

    /// The second liveness proof, on a real repository file rather than a
    /// string in this module: `tests/fixtures/lint_golden/broad.lisp` writes
    /// `(return-from blk nil)` at top level, with no `blk` anywhere, and this
    /// batch's first rule sees it.
    ///
    /// Paired with the sweep above, the two together say the harness reads
    /// repository files, and that reading them can produce both answers.
    #[test]
    fn the_lint_golden_fixtures_do_trip_a_rule() {
        let files = repository_lisp_files(&["tests/fixtures/lint_golden"]);
        assert!(!files.is_empty(), "no lint_golden fixture was read");
        let fired: Vec<&'static str> = files
            .iter()
            .flat_map(|path| findings_in(&std::fs::read_to_string(path).expect("read fixture")))
            .collect();
        assert!(
            fired.contains(&"return-from-unmatched-block"),
            "expected the deliberately-defective fixtures to trip a rule, got {fired:?}"
        );
    }

    /// The repo's `.lisp` files under `directories`, or an empty list when
    /// this crate is being built somewhere the repository root is not above
    /// it.
    fn repository_lisp_files(directories: &[&str]) -> Vec<PathBuf> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let mut found = Vec::new();
        for directory in directories {
            let Ok(entries) = std::fs::read_dir(root.join(directory)) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path
                    .extension()
                    .is_some_and(|extension| extension == "lisp")
                {
                    found.push(path);
                }
            }
        }
        found.sort();
        found
    }
}
