//! `check-then-act`: testing a place and then writing it.
//!
//! `(unless (gethash key table) (setf (gethash key table) value))` reads a
//! table, decides, and writes — three steps that another thread can interleave
//! between. The window is small and the failure is a silent overwrite, which is
//! the combination that makes it survive review.
//!
//! Detection is exact rather than heuristic: the *same place expression* must
//! appear in the test and in the assignment. Comparing the two source forms
//! structurally is what rules out the enormous number of unrelated
//! test-then-assign pairs that are perfectly fine.
//!
//! Report-only. The remedy is an atomic primitive or a lock, both of which are
//! implementation-specific and neither of which a rule may choose.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head, symbol_in};

pub const META: RuleMeta = RuleMeta::new(
    "check-then-act",
    RuleCategory::Concurrency,
    Severity::Warning,
    "a conditional that tests a place and then assigns to the same place, which another thread can interleave",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "Reading a shared place, deciding on the result, and then writing it is three operations. \
         A second thread that runs between the read and the write sees the same 'not present' \
         answer, and one of the two writes is silently lost.",
    )
    .with_example(
        "(unless (gethash k table) (setf (gethash k table) v))",
        "(sb-ext:with-locked-hash-table (table) (unless (gethash k table) (setf (gethash k table) v)))",
    )
    .with_caveat(
        "The test and the assignment must name the *same* place, compared structurally. A \
         conditional whose test happens to mention a different key or a different table is not \
         reported.",
    ),
);

const HEADS: [NormalizedHead; 4] = [
    NormalizedHead::new("unless"),
    NormalizedHead::new("when"),
    NormalizedHead::new("if"),
    NormalizedHead::new("cond"),
];

/// Places a check-then-act window is worth reporting for: shared, mutable, and
/// reachable by more than one thread.
const SHARED_PLACES: [&str; 5] = ["gethash", "get", "getf", "symbol-value", "aref"];

/// One check-then-act window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckThenAct {
    pub span: ByteSpan,
    /// The place accessor both halves name.
    pub place: String,
}

/// Whether two expressions are the same form, ignoring only whitespace.
///
/// Structural rather than textual: `(gethash k  table)` and `(gethash k table)`
/// are the same place, and a source-text comparison would miss the pair when
/// the two halves are formatted differently — which, spanning a line break, is
/// the usual case.
#[must_use]
pub fn same_form(left: &ExpressionView, right: &ExpressionView) -> bool {
    match (atom_text(left), atom_text(right)) {
        (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
        (None, None) => {
            left.children.len() == right.children.len()
                && left
                    .children
                    .iter()
                    .zip(&right.children)
                    .all(|(a, b)| same_form(a, b))
        }
        _ => false,
    }
}

/// The shared place a test expression reads, looking through the `null`/`not`
/// wrappers a presence check is usually spelled with.
fn tested_place(test: &ExpressionView) -> Option<&ExpressionView> {
    let head = list_head(test)?;
    if symbol_in(head, &["null", "not"]) {
        return test.children.get(1).and_then(tested_place);
    }
    symbol_in(head, &SHARED_PLACES).then_some(test)
}

/// Reads one conditional and reports the check-then-act window it opens.
#[must_use]
pub fn examine(view: &ExpressionView) -> Option<CheckThenAct> {
    let head = list_head(view)?;
    if !symbol_in(head, &["unless", "when", "if", "cond"]) {
        return None;
    }
    // `cond` carries its test inside each clause; the other three carry it
    // directly. Reading the first clause covers the shape that occurs.
    let (test, body): (&ExpressionView, &[ExpressionView]) = if symbol_in(head, &["cond"]) {
        let clause = view.children.get(1)?;
        (clause.children.first()?, clause.children.get(1..)?)
    } else {
        (view.children.get(1)?, view.children.get(2..)?)
    };

    let place = tested_place(test)?;
    let accessor = list_head(place)?.to_owned();

    let mut found = false;
    for form in body {
        for_each_subview(form, |subview| {
            if found {
                return;
            }
            let Some(operator) = list_head(subview) else {
                return;
            };
            if !symbol_in(
                operator,
                &["setf", "setq", "incf", "decf", "push", "pushnew"],
            ) {
                return;
            }
            found = subview
                .children
                .iter()
                .skip(1)
                .any(|argument| same_form(argument, place));
        });
    }

    found.then(|| CheckThenAct {
        span: view.span,
        place: paredit_core_syntax::view_query::unqualified(&accessor).to_ascii_lowercase(),
    })
}

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn check(
        &self,
        _context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        if let Some(found) = examine(view) {
            sink.report(
                found.span,
                format!(
                    "this tests a {} place and then assigns to the same place; another thread can \
                     run between the two and one write is lost",
                    found.place
                ),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::{Path, SyntaxTree};

    fn place(input: &str) -> Option<String> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let view = tree
            .select_path(&Path::root_child(0))
            .expect("root form")
            .view();
        examine(&view).map(|found| found.place)
    }

    #[test]
    fn flags_the_canonical_memoisation_window() {
        assert_eq!(
            place("(unless (gethash k table) (setf (gethash k table) v))"),
            Some("gethash".to_owned())
        );
    }

    #[test]
    fn sees_through_a_null_or_not_wrapper() {
        assert_eq!(
            place("(when (null (gethash k table)) (setf (gethash k table) v))"),
            Some("gethash".to_owned())
        );
        assert_eq!(
            place("(if (not (gethash k table)) (setf (gethash k table) v))"),
            Some("gethash".to_owned())
        );
    }

    #[test]
    fn reads_a_cond_clause_too() {
        assert_eq!(
            place("(cond ((null (gethash k table)) (setf (gethash k table) v)))"),
            Some("gethash".to_owned())
        );
    }

    #[test]
    fn does_not_flag_a_different_place() {
        assert_eq!(
            place("(unless (gethash k table) (setf (gethash other table) v))"),
            None
        );
        assert_eq!(
            place("(unless (gethash k table) (setf (gethash k backup) v))"),
            None
        );
    }

    #[test]
    fn compares_structurally_not_textually() {
        assert_eq!(
            place("(unless (gethash k table)\n  (setf (gethash  k   table) v))"),
            Some("gethash".to_owned())
        );
    }

    #[test]
    fn does_not_flag_a_test_of_something_that_is_not_a_shared_place() {
        assert_eq!(place("(unless (ready-p x) (setf state :go))"), None);
    }

    #[test]
    fn does_not_flag_a_conditional_that_writes_nothing() {
        assert_eq!(place("(unless (gethash k table) (warn \"missing\"))"), None);
    }

    #[test]
    fn finds_the_assignment_nested_in_the_body() {
        assert_eq!(
            place("(unless (gethash k table) (progn (log) (setf (gethash k table) v)))"),
            Some("gethash".to_owned())
        );
    }

    #[test]
    fn same_form_ignores_case_and_reads_structure() {
        let tree = SyntaxTree::parse_with_dialect("(f (g X) 1)", Dialect::CommonLisp).unwrap();
        let left = tree.select_path(&Path::root_child(0)).unwrap().view();
        let tree = SyntaxTree::parse_with_dialect("(F (G x) 1)", Dialect::CommonLisp).unwrap();
        let right = tree.select_path(&Path::root_child(0)).unwrap().view();
        assert!(same_form(&left, &right));

        let tree = SyntaxTree::parse_with_dialect("(f (g x))", Dialect::CommonLisp).unwrap();
        let shorter = tree.select_path(&Path::root_child(0)).unwrap().view();
        assert!(!same_form(&left, &shorter));
    }
}
