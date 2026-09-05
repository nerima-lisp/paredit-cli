//! `with-open-returns-lazy-seq`: a resource scope whose value is realized after
//! the resource closes.
//!

use paredit_core_lint_engine::LintResult;

use crate::support::is_unevaluated_at;
use crate::with_open_returns_lazy_seq::domain::examine_with_open;
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "with-open-returns-lazy-seq",
    // The resource is the subject: a stream read after it closed. The laziness
    // is how it happens, not what is wrong.
    RuleCategory::Resource,
    // Not a style preference — the realized sequence throws `IOException:
    // Stream closed`, and only on inputs large enough not to have been buffered.
    Severity::Error,
    "a with-open whose value is a lazy sequence over the resource it closes",
    // The repair is a choice: realize with `doall`, collect with `into`, fold
    // with `reduce`, or restructure so the caller owns the scope. Each has a
    // different memory profile, and picking one would be picking for the author.
    Fixability::ReportOnly,
);

/// `examine_with_open` only ever matches this head.
///
/// Anchoring on the *scope* rather than on the lazy vocabulary is deliberate
/// and is the rule's whole cost story: `map`, `filter` and `for` are on nearly
/// every line of Clojure, and a rule keyed on them would run its body
/// constantly and then have to walk *upwards* to find out whether a `with-open`
/// encloses it. `with-open` appears a handful of times per file, and everything
/// the rule needs is underneath it.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("with-open")];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    /// The trait default is Common Lisp only, which has no `with-open`, no
    /// `line-seq`, and no lazy sequences to be caught by the close.
    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::CLOJURE_ONLY
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut scope_count = 0;
        let mut items = Vec::new();
        examine_with_open(view, &mut scope_count, &mut items);
        if items.is_empty() {
            return Ok(());
        }
        // Only once a candidate exists: `is_unevaluated_at` materializes the
        // enclosing top-level form, so asking it first would charge every
        // `with-open` in the file for a question almost none of them need.
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        for item in items {
            sink.report(item.span, item.message());
        }
        Ok(())
    }
}
