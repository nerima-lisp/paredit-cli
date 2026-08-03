//! `apply-with-literal-collection`: an argument list written out and then
//! spread.
//!
//! The analysis lives in [`crate::apply_with_literal_collection::domain`],
//! which also backs the standalone `inspect apply-with-literal-collection`
//! command; this module only registers it with the lint suite and phrases its
//! findings.

use paredit_core_lint_engine::LintResult;

use crate::apply_with_literal_collection::domain::examine_apply;
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
    "apply-with-literal-collection",
    // A vector built only to be taken apart again on the next instruction.
    RuleCategory::Allocation,
    // The code is correct, and the cost is one short-lived vector. This is a
    // legibility complaint with an allocation attached, not a bug.
    Severity::Warning,
    "an apply whose argument sequence is a literal, so the call can be written directly",
    // Mechanically the rewrite is `(apply f [a b])` → `(f a b)`, but it is a
    // structural splice rather than a token substitution, and this package
    // ships no fixes; see the README.
    Fixability::ReportOnly,
);

/// `examine_apply` only ever matches this head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("apply")];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    /// The trait default is Common Lisp only. Common Lisp's `apply` has the
    /// same shape, but not the same literal spellings — `#(1 2 3)` is a vector
    /// there and a reader lambda here — so the check does not transfer, and
    /// the Common Lisp rules live in their own packages.
    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::CLOJURE_ONLY
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut apply_count = 0;
        let mut items = Vec::new();
        examine_apply(view, &mut apply_count, &mut items);
        if items.is_empty() {
            return Ok(());
        }
        // Only once a candidate exists: `is_unevaluated_at` materializes the
        // enclosing top-level form, and `apply` is common enough that asking it
        // per head match would be a per-call cost on ordinary code.
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        for item in items {
            sink.report(item.span, item.message());
        }
        Ok(())
    }
}
