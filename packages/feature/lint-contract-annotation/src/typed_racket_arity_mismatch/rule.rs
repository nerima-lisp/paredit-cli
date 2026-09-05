//! `typed-racket-arity-mismatch`: a `(: name (-> …))` annotation whose arrow
//! arity disagrees with the `define` below it.
//!

use paredit_core_lint_engine::LintResult;

use crate::typed_racket_arity_mismatch::domain::{SCOPE, examine_define, is_data_at};
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "typed-racket-arity-mismatch",
    // A call with an argument count the operator cannot accept is `Arity`, and
    // this is the declaration half of exactly that: every caller type-checked
    // against this annotation is checked against the wrong number.
    RuleCategory::Arity,
    Severity::Warning,
    "a Typed Racket (: name (-> ...)) annotation whose arrow arity disagrees with the define",
    // Which half is wrong — the annotation or the parameter list — is the
    // author's call, and there is no repair that is right more often than not.
    Fixability::ReportOnly,
);

/// `define` is the anchor because the engine's head index *cannot* be keyed on
/// `:`: [`NormalizedHead`] rejects any head containing a colon at compile time,
/// so no rule can ask to be shown `(: …)` forms. The annotation is reached by
/// looking one top-level sibling up from the `define` instead — a binary search
/// over the top level, never a scan.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("define")];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    /// There is no `RACKET_ONLY` constant to reach for — this is the first
    /// built-in rule scoped to Racket alone — so the scope is constructed
    /// explicitly in the domain and read from there, which is also where the
    /// standalone report's `dialect_modelled` flag comes from.
    fn dialect_scope(&self) -> RuleDialectScope {
        SCOPE
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut annotated_define_count = 0;
        let mut items = Vec::new();
        examine_define(
            context.tree(),
            view,
            &mut annotated_define_count,
            &mut items,
        );
        if items.is_empty() {
            return Ok(());
        }
        // Asked once per candidate, after the head has matched and a finding is
        // already in hand.
        if is_data_at(context.tree(), view.span) {
            return Ok(());
        }
        for item in items {
            sink.report(item.span, item.message());
        }
        Ok(())
    }
}
