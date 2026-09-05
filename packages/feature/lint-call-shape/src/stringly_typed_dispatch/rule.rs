//! `stringly-typed-dispatch`: a `cond`/`if` chain dispatching on string
//! equality against a set of identifier-shaped literals.
//!

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, RuleSetting,
    RuleTag, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::stringly_typed_dispatch::domain::{
    DEFAULT_MIN_BRANCHES, MODELLED_DIALECTS, examine_dispatch, message,
};

/// The knob: how many same-subject string branches make a set read as an
/// enumeration.
pub const MIN_BRANCHES: RuleSetting = RuleSetting::new(
    "min-branches",
    DEFAULT_MIN_BRANCHES as i64,
    "how many string-equality branches on one subject a form needs before it is reported",
);

pub const META: RuleMeta = RuleMeta::new(
    "stringly-typed-dispatch",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a cond/if chain dispatching on string equality against an enumeration of literals",
    Fixability::ReportOnly,
)
.with_tags(&[RuleTag::Style, RuleTag::Pedantic])
.with_settings(&[MIN_BRANCHES])
.with_explanation(
    RuleExplanation::new(
        "A set of short string literals compared for equality is an enumeration written as text. \
         Nothing checks it: a misspelt literal compiles, reads fine, and silently falls through. \
         Interning the value once at the boundary turns the same dispatch into a `case`, where a \
         typo becomes a clause that is visibly never taken.",
    )
    .with_example(
        "(cond ((string= m \"read\") …) ((string= m \"write\") …))",
        "(case m (:read …) (:write …))",
    )
    .with_caveat(
        "Every counted branch must compare the *same* subject against a *distinct*, \
         identifier-shaped literal. Comparisons of different subjects, repeated literals, and \
         literals containing spaces or format directives are not counted, so a `cond` that is \
         genuinely comparing text is never reported.",
    ),
);

/// Exactly the heads `examine_dispatch` reads.
const HEADS: [NormalizedHead; 2] = [NormalizedHead::new("cond"), NormalizedHead::new("if")];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    /// Common Lisp and Emacs Lisp spell `cond`, `if`, `string=` and
    /// `string-equal` identically and give them the same meaning. Clojure's `=`
    /// and Scheme's `string=?` are different spellings, and claiming them here
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
        let min_branches = context.setting(META.name().as_str(), MIN_BRANCHES).max(0) as usize;
        let mut dispatch_form_count = 0;
        let mut items = Vec::new();
        examine_dispatch(
            context.tree(),
            view,
            min_branches,
            &mut dispatch_form_count,
            &mut items,
        );
        for item in items {
            sink.report(
                item.span,
                message(item.form, &item.subject, item.branch_count),
            );
        }
        Ok(())
    }
}
