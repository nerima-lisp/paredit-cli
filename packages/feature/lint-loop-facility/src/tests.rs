//! Suite-level tests: the whole crate against realistic code.
//!
//! The per-rule pairs live beside each rule's `domain.rs`. What those cannot
//! carry is the weight here — a realistic *correct* file sweeps to zero
//! findings while also asserting a non-zero count of the shapes each rule keys
//! on, so it cannot pass by matching nothing, and a dangerous twin asserts one
//! finding per rule.
//!
//! Every test dispatches through [`collect_lint_outcomes`], never through a
//! rule's `check`. Calling `check` directly bypasses the head index, which is
//! where a wrong `HeadFilter` or a forgotten `dialect_scope` shows up. Tests
//! that call `check` directly cannot detect a missing `Heads` entry.

use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
use paredit_core_lint_engine::model::LintOutcome;
use paredit_core_lint_engine::policy::RuleSelection;
use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ExpressionView, SyntaxTree};
use std::path::Path;

use crate::accumulation_discarded_by_finally_return as discarded;
use crate::into_accumulator_never_read as unread;
use crate::parallel_binding_reads_sibling as parallel;

/// The rules of this crate, in one catalogue, so a test run dispatches through
/// the real engine.
const CATALOG: [RuleEntry; 3] = [
    RuleEntry::new(&parallel::rule::META, &parallel::rule::RULE),
    RuleEntry::new(&unread::rule::META, &unread::rule::RULE),
    RuleEntry::new(&discarded::rule::META, &discarded::rule::RULE),
];

const PARALLEL: &str = "loop-parallel-binding-reads-sibling";
const UNREAD: &str = "loop-into-accumulator-never-read";
const DISCARDED: &str = "loop-accumulation-discarded-by-finally-return";
const NONE: [&str; 0] = [];

fn outcomes_in(dialect: Dialect, source: &str) -> Vec<LintOutcome> {
    let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("fixture parses");
    let catalog = RuleCatalog::new(&CATALOG);
    let index = build_head_index(catalog);
    collect_lint_outcomes(
        catalog,
        &index,
        Path::new("f.lisp"),
        dialect,
        &tree,
        source,
        RuleSelection::All,
    )
    .expect("the engine runs")
}

/// The rule names that fire on `source`, as Common Lisp.
fn rules_for(source: &str) -> Vec<&'static str> {
    let mut names: Vec<&'static str> = outcomes_in(Dialect::CommonLisp, source)
        .into_iter()
        .map(|outcome| outcome.into_parts().0.rule)
        .collect();
    names.sort_unstable();
    names
}

/// Every `loop` form in a source, for the candidate counts below.
fn each_loop(source: &str, mut visit: impl FnMut(&ExpressionView)) {
    let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parses");
    fn walk(view: &ExpressionView, visit: &mut impl FnMut(&ExpressionView)) {
        if crate::loop_grammar::is_loop_form(view) {
            visit(view);
        }
        for child in &view.children {
            walk(child, visit);
        }
    }
    walk(&tree.root_view(), &mut visit);
}

fn candidates(source: &str) -> (usize, usize, usize) {
    let (mut a, mut b, mut c) = (0, 0, 0);
    each_loop(source, |view| {
        a += parallel::domain::candidate_count(view);
        b += unread::domain::candidate_count(view);
        c += discarded::domain::candidate_count(view);
    });
    (a, b, c)
}

// ---------------------------------------------------------------------------
// The permanent corpus: realistic *correct* Common Lisp
// ---------------------------------------------------------------------------

/// A file of ordinary, correct `loop` code — including every shape each rule
/// keys on, written the right way.
///
/// The shapes here are the ones the third-party sweep over SBCL's own sources
/// and 38 Quicklisp systems showed to be common: parallel `and` groups that do
/// not cross-reference, the `and prev = nil then x` previous-element idiom,
/// `into` accumulators returned from `finally`, and implicit accumulations with
/// no `finally` at all.
const CORRECT: &str = r#";;;; correct.lisp -- ordinary loop code, all of it right.

(defun pair-up (xs ys)
  ;; A parallel group with no cross-reference: the everyday use of `and`.
  (loop for x in xs
        and y in ys
        collect (cons x y)))

(defun successive (limit)
  ;; Sequential binding, where reading the earlier variable is correct.
  (loop for a from 1 to limit
        for b = (* a 10)
        collect (list a b)))

(defun deltas (values)
  ;; The previous-element idiom. This *requires* `and`, and the sibling read
  ;; sits in the `then` step form.
  (loop for x in values
        and prev = nil then x
        when prev collect (- x prev)))

(defun tally (items)
  ;; An `into` accumulator that `finally` returns.
  (loop for item in items
        count (evenp item) into evens
        collect item into all
        finally (return (values evens all))))

(defun summarize (rows)
  ;; An implicit accumulation with no `finally` at all.
  (loop for row in rows
        when (consp row)
          collect (car row)))

(defun scan-table (table)
  ;; An iteration path, which the reader declines to model outright.
  (loop for key being the hash-keys of table
        using (hash-value value)
        collect (cons key value)))

(defmacro with-collected ((var) &body body)
  ;; A `loop` in a macro template is the expansion's code, not this macro's.
  `(loop for ,var in items
         and other = (compute ,var)
         collect other))

(defun keyword-named-variables (count end)
  ;; Variables spelled like clause keywords, bound outside and read inside.
  (loop for i from 1 to count
        for j from 0 below end
        collect (list i j)))

(defun stepping (plist)
  ;; `by #'cddr` is the idiomatic plist walk, not a defect.
  (loop for tail on plist by #'cddr
        collect (car tail)))
"#;

#[test]
fn realistic_correct_code_yields_no_findings() {
    assert_eq!(rules_for(CORRECT), NONE);
}

/// The other half of the corpus test, and the half that matters. A zero-finding
/// sweep over zero candidates is a false clean — it would pass just as well if
/// every rule were `return Ok(())`.
#[test]
fn the_correct_corpus_actually_exercises_every_rule() {
    let (parallel_groups, into_clauses, implicit_accumulations) = candidates(CORRECT);
    assert!(
        parallel_groups >= 2,
        "the corpus must contain parallel `and` groups; found {parallel_groups}"
    );
    assert!(
        into_clauses >= 2,
        "the corpus must contain `into` accumulations; found {into_clauses}"
    );
    assert!(
        implicit_accumulations >= 3,
        "the corpus must contain implicit accumulations; found {implicit_accumulations}"
    );
}

// ---------------------------------------------------------------------------
// The dangerous twin
// ---------------------------------------------------------------------------

/// The same file with each rule's defect introduced exactly once.
const DANGEROUS_TWIN: &str = r#";;;; twin.lisp -- every rule in this crate fires here, once.

(defun scaled (limit)
  ;; `b` reads `a`, which is bound in parallel with it: SBCL returns
  ;; ((1 10) (2 10) (3 20)) with no warning.
  (loop for a from 1 to limit
        and b = (* a 10)
        collect (list a b)))

(defun tally (items)
  ;; `evens` is accumulated and never read, so this returns nil.
  (loop for item in items
        count (evenp item) into evens))

(defun summarize (rows)
  ;; The collected list is fully consed and then thrown away.
  (loop for row in rows
        collect (car row)
        finally (return :done)))
"#;

#[test]
fn the_dangerous_twin_fires_each_rule_exactly_once() {
    // Sorted, because `rules_for` sorts: the engine's dispatch order is not
    // this test's subject.
    assert_eq!(rules_for(DANGEROUS_TWIN), [DISCARDED, UNREAD, PARALLEL]);
}

/// Every fixture in this file must parse, so a rule can never look clean
/// because the reader gave up.
#[test]
fn every_fixture_parses() {
    for source in [CORRECT, DANGEROUS_TWIN] {
        assert!(SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).is_ok());
    }
}

// ---------------------------------------------------------------------------
// Dialect scope and dispatch
// ---------------------------------------------------------------------------

/// The rules are Common Lisp only. Clojure has a `loop` of its own with a
/// completely different grammar, and reading it with this one would be
/// nonsense.
#[test]
fn no_rule_fires_on_another_dialect() {
    for dialect in [Dialect::Clojure, Dialect::Scheme, Dialect::EmacsLisp] {
        let found: Vec<&str> = outcomes_in(dialect, DANGEROUS_TWIN)
            .into_iter()
            .map(|outcome| outcome.into_parts().0.rule)
            .collect();
        assert_eq!(found, NONE, "fired on {dialect:?}");
    }
}

/// A `loop` reached only as quoted data is never reported, at any depth. This
/// dispatches through the engine, so it also proves the `is_unevaluated_at`
/// guard in each rule's `check` is actually wired up.
#[test]
fn a_loop_reached_only_as_data_is_never_reported() {
    for source in [
        "(defparameter *f* '(loop for a from 1 to 3 and b = (* a 10) collect b))",
        "(defparameter *f* '(progn (loop for x in items collect x into acc)))",
        "(defmacro m () `(loop for x in items collect x finally (return :done)))",
        "(defmacro m (&body body) `(progn (loop for a from 1 to 3 and b = (* a 10) collect b) ,@body))",
    ] {
        assert_eq!(rules_for(source), NONE, "reported data: {source}");
    }
}

/// The complement of the test above: the same shapes, evaluated, do fire. Two
/// tests that differ by exactly the quote.
#[test]
fn the_same_shapes_evaluated_do_fire() {
    assert_eq!(
        rules_for("(defun f () (loop for a from 1 to 3 and b = (* a 10) collect b))"),
        [PARALLEL]
    );
    assert_eq!(
        rules_for("(defun f () (loop for x in items collect x into acc))"),
        [UNREAD]
    );
    assert_eq!(
        rules_for("(defun f () (loop for x in items collect x finally (return :done)))"),
        [DISCARDED]
    );
}

/// A quasiquoted template with an unquote escapes back to code, and the rule
/// must follow it there. This is the case a single depth counter gets wrong.
#[test]
fn an_unquoted_loop_inside_a_template_is_still_code() {
    assert_eq!(
        rules_for("(defmacro m () `(progn ,(loop for a from 1 to 3 and b = (* a 10) collect b)))"),
        [PARALLEL]
    );
}

/// Reader conditionals fold into a single opaque atom under this workspace's
/// dialect-aware parse, which shifts child indices. A `loop` written beside one
/// must still read correctly rather than silently mis-indexing.
#[test]
fn a_reader_conditional_beside_a_loop_does_not_shift_the_reading() {
    let source =
        "(defun f ()\n  #+sbcl (sb-ext:gc)\n  (loop for a from 1 to 3 and b = (* a 10) collect b))";
    assert_eq!(rules_for(source), [PARALLEL]);
}
