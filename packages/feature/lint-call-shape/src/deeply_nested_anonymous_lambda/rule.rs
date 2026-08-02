//! `deeply-nested-anonymous-lambda`: three or more anonymous lambdas nested
//! with no name in between.
//!
//! The analysis lives in [`crate::deeply_nested_anonymous_lambda::domain`],
//! which also backs the standalone
//! `inspect deeply-nested-anonymous-lambda` command; this module only registers
//! it with the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, RuleSetting,
    RuleTag, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::deeply_nested_anonymous_lambda::domain::{
    DEFAULT_MAX_NESTING, MODELLED_DIALECTS, examine_lambda, message,
};

/// The knob: how many levels of anonymous nesting are acceptable. The first
/// level *reported* is one more than this.
pub const MAX_NESTING: RuleSetting = RuleSetting::new(
    "max-nesting",
    DEFAULT_MAX_NESTING as i64,
    "how many anonymous lambdas may nest inside one another before it is reported",
);

pub const META: RuleMeta = RuleMeta::new(
    "deeply-nested-anonymous-lambda",
    RuleCategory::Suspicious,
    Severity::Warning,
    "three or more anonymous lambdas nested with no intervening named binding",
    Fixability::ReportOnly,
)
.with_tags(&[RuleTag::Style, RuleTag::Pedantic])
.with_settings(&[MAX_NESTING])
.with_explanation(
    RuleExplanation::new(
        "Every step of a nested lambda chain is spelled only by its position, so following the \
         data flow means re-reading the whole expression. Naming the intermediate steps — a \
         `let`, an `flet`, a top-level helper — costs one line and gives a reader something to \
         hold on to.",
    )
    .with_example(
        "(lambda (f) (lambda (g) (lambda (x) (funcall f (funcall g x)))))",
        "(defun compose (f g) (lambda (x) (funcall f (funcall g x))))",
    )
    .with_caveat(
        "A named intermediate breaks the chain: `(lambda (x) (let ((step (lambda (y) …))) …))` \
         is two chains of one and is never reported, however many `let`-bound lambdas follow.",
    ),
);

/// `examine_lambda` only ever matches a `lambda` head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("lambda")];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    /// `lambda`, `let`, `flet` and `defun` all mean here what the chain logic
    /// assumes in Common Lisp and in Emacs Lisp. Clojure's `fn`, Scheme's named
    /// `let` and Racket's `define` bind differently enough that including them
    /// would be a guess rather than a scope.
    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::new(&MODELLED_DIALECTS)
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let max_nesting = context.setting(META.name().as_str(), MAX_NESTING).max(0) as usize;
        let mut lambda_form_count = 0;
        let mut items = Vec::new();
        examine_lambda(
            context.tree(),
            view,
            max_nesting,
            &mut lambda_form_count,
            &mut items,
        );
        for item in items {
            sink.report(item.span, message(item.nesting_depth, item.threshold));
        }
        Ok(())
    }
}
