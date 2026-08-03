//! `contains-on-non-associative`: a membership test whose answer is fixed
//! before it runs.
//!
//! The analysis lives in
//! [`crate::contains_on_non_associative::domain`], which also backs the
//! standalone `inspect contains-on-non-associative` command; this module only
//! registers it with the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::contains_on_non_associative::domain::examine_contains;
use crate::support::is_unevaluated_at;
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "contains-on-non-associative",
    // A predicate whose answer does not depend on its arguments: it reads as a
    // membership test and is not one.
    RuleCategory::Suspicious,
    // One shape throws `IllegalArgumentException`; the other is silently and
    // permanently `false`, which is the worse of the two.
    Severity::Error,
    "a contains? whose collection is a sequence, or a literal vector asked about a non-index key, so the call can never answer true",
    // `(some #{k} coll)`, a `set`, or restructuring the data are three
    // different repairs with three different costs; this package ships no
    // fixes.
    Fixability::ReportOnly,
);

/// `examine_contains` only ever matches this head.
///
/// The same name as `domain::CONTAINS_HEADS`, in the form the dispatcher's
/// index is keyed by. The package's `engine_pass_tests` is what pins the two
/// together.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("contains?")];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    /// The trait default is Common Lisp only, which has no `contains?` and no
    /// `[…]` vector literal.
    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::CLOJURE_ONLY
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut contains_count = 0;
        let mut items = Vec::new();
        examine_contains(view, &mut contains_count, &mut items);
        if items.is_empty() {
            return Ok(());
        }
        // Only once a candidate exists; see `crate::support`'s cost section.
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        for item in items {
            sink.report(item.span, item.message());
        }
        Ok(())
    }
}
