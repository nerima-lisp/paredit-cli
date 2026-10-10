//! `declarative-style-score`: the fraction of top-level forms whose heads
//! are recognized definitions, declarations, or value bindings.
//!
//! This is a syntactic style metric, not an effect or load-order analysis.
//! Initializers are counted even when they call effectful code. Definition
//! forms can also depend on the environment or change it. A high score does
//! not prove that reordering, splitting, or reloading preserves behavior.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, RuleCategory, RuleExplanation, RuleMeta, RuleSetting, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;
use paredit_core_syntax::view_query::{list_head, symbol_in};

const DEFINITION_HEADS: [&str; 12] = [
    "defun",
    "defmacro",
    "defgeneric",
    "defmethod",
    "defclass",
    "defstruct",
    "define-condition",
    "deftype",
    "defpackage",
    "in-package",
    "declaim",
    "declare",
];

/// Initializer effects do not change this head-based classification.
const VALUE_BINDING_HEADS: [&str; 3] = ["defvar", "defparameter", "defconstant"];

/// The minimum percentage (0-100) of recognized declaration-style heads.
pub const MIN_SCORE: RuleSetting = RuleSetting::new(
    "min-score",
    80,
    "the minimum percentage of top-level forms with recognized declaration-style heads",
);

/// How many top-level forms a file needs before a score means anything. A
/// one-form file is either 0% or 100% declarative, which is not a signal
/// worth reporting; a short script or a test fixture should not be judged the
/// same way a multi-hundred-form source file is.
const MIN_JUDGED_FORMS: usize = 5;

pub const META: RuleMeta = RuleMeta::new(
    "declarative-style-score",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a file whose fraction of recognized declaration-style heads falls below a threshold",
    Fixability::ReportOnly,
)
.with_settings(&[MIN_SCORE])
.with_explanation(
    RuleExplanation::new(
        "The score counts top-level forms with recognized definition, declaration, or value-binding \
         heads. A low score identifies files with more other top-level forms for style review.",
    )
    .with_example(
        "(defun a () ...)\n(a)\n(defun b () ...)\n(b)\n(setf *ready* t)",
        "(defun a () ...)\n(defun b () ...)\n(defun main () (a) (b) (setf *ready* t))",
    )
    .with_caveat(
        "The score does not inspect initializer effects, macro expansion, or environment dependencies. \
         Even a score of 100% does not prove purity or safe reordering, splitting, or reloading. \
         Moving calls into a function also changes when they run and requires manual review.",
    ),
);

/// The percentage (0-100) of recognized declaration-style heads, and the
/// number of top-level forms counted. `None` when the
/// file has fewer than [`MIN_JUDGED_FORMS`] top-level forms — too few for a
/// percentage to mean anything.
#[must_use]
pub fn declarative_score(root: &ExpressionView) -> Option<(u32, usize)> {
    let total = root.children.len();
    if total < MIN_JUDGED_FORMS {
        return None;
    }
    let declarative = root
        .children
        .iter()
        .filter(|form| {
            list_head(form).is_some_and(|head| {
                symbol_in(head, &DEFINITION_HEADS) || symbol_in(head, &VALUE_BINDING_HEADS)
            })
        })
        .count();
    #[allow(clippy::cast_possible_truncation)]
    let score = ((declarative as f64 / total as f64) * 100.0).round() as u32;
    Some((score, total))
}

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        // The score is computed over every top-level form at once.
        HeadFilter::WholeTree
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        let Some((score, total)) = declarative_score(view) else {
            return Ok(());
        };
        let threshold = context
            .setting(META.name().as_str(), MIN_SCORE)
            .clamp(0, 100) as u32;
        if score < threshold {
            sink.report(
                view.span,
                format!(
                    "this file's declarative-style score is {score}% ({total} top-level forms), \
                     below the {threshold}% threshold"
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
    use paredit_core_syntax::sexpr::SyntaxTree;

    fn score(input: &str) -> Option<(u32, usize)> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        declarative_score(&tree.root_view())
    }

    #[test]
    fn a_purely_declarative_file_scores_100() {
        assert_eq!(
            score(
                "(defun a () 1)\n(defvar *x* 1)\n(defclass c () ())\n\
                 (defconstant +k+ 1)\n(deftype d () 'integer)"
            ),
            Some((100, 5))
        );
    }

    #[test]
    fn a_file_of_bare_calls_scores_0() {
        assert_eq!(score("(a)\n(b)\n(c)\n(d)\n(e)"), Some((0, 5)));
    }

    #[test]
    fn a_mixed_file_scores_the_declarative_fraction() {
        // 3 of 6 top-level forms (the three defuns) are declarative.
        assert_eq!(
            score("(defun a () 1)\n(a)\n(defun b () 1)\n(b)\n(defun c () 1)\n(c)"),
            Some((50, 6))
        );
    }

    #[test]
    fn an_empty_file_has_no_score() {
        assert_eq!(score(""), None);
    }

    #[test]
    fn a_file_with_too_few_forms_has_no_score() {
        // Below MIN_JUDGED_FORMS: a one-form file is trivially 0% or 100%,
        // which is not a signal worth reporting.
        assert_eq!(score("(a)\n(b)\n(c)\n(d)"), None);
    }

    #[test]
    fn a_defparameter_counts_as_declarative_even_with_a_computed_value() {
        assert_eq!(
            score("(defparameter *x* (+ 1 2))\n(a)\n(b)\n(c)\n(d)"),
            Some((20, 5))
        );
    }

    #[test]
    fn effectful_initializers_can_score_100_without_proving_purity() {
        assert_eq!(
            score(
                "(defparameter *a* (write-line \"a\"))\n\
                   (defvar *b* (read))\n\
                   (defconstant +c+ (random 10))\n\
                   (defclass d (external-base) ())\n\
                   (in-package :external)"
            ),
            Some((100, 5))
        );
    }
}
