//! `clojure-pre-post-vacuous`: a `:pre`/`:post` vector that asserts nothing.
//!

use paredit_core_lint_engine::LintResult;

use crate::clojure_pre_post_vacuous::domain::{SCOPE, examine_defn, is_data_at};
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "clojure-pre-post-vacuous",
    // Well-formed code whose meaning is probably not what was intended: the
    // author wrote a contract, and the contract asserts nothing.
    RuleCategory::Suspicious,
    Severity::Warning,
    "a defn :pre/:post vector that is empty or all literal true, so it asserts nothing",
    // Deleting the vector and writing the condition the author meant are two
    // different edits, and only the author knows which was intended.
    Fixability::ReportOnly,
);

/// `examine_defn` only ever matches these two heads. Clojure's `fn` accepts the
/// same condition map, but its parameter vector is optional in a way that makes
/// the lone-map question harder; a `defn` is what production code writes.
const HEADS: [NormalizedHead; 2] = [NormalizedHead::new("defn"), NormalizedHead::new("defn-")];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    /// The trait default is Common Lisp only, which has no `defn` and no
    /// condition map. Read from the same constant the standalone report's
    /// `dialect_modelled` flag uses, so the two cannot drift.
    fn dialect_scope(&self) -> RuleDialectScope {
        SCOPE
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut contract_count = 0;
        let mut items = Vec::new();
        examine_defn(view, &mut contract_count, &mut items);
        if items.is_empty() {
            return Ok(());
        }
        // Asked once per candidate, and only after the head has matched and a
        // finding is already in hand: the dispatcher hands a rule quoted data
        // too, and a `defn` inside a macro template is not code.
        if is_data_at(context.tree(), view.span) {
            return Ok(());
        }
        for item in items {
            sink.report(item.span, item.message());
        }
        Ok(())
    }
}
