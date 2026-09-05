//! `reference-type-operator-mismatch`: an atom operator on a ref, and the
//! other five crossings.
//!

use paredit_core_lint_engine::LintResult;

use crate::reference_type_operator_mismatch::domain::examine_reference_bindings;
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
    "reference-type-operator-mismatch",
    // The four reference types exist only to be shared between threads, and
    // their operators differ because their coordination guarantees do.
    RuleCategory::Concurrency,
    // `ClassCastException` on the first call. Not a matter of taste.
    Severity::Error,
    "an atom/ref/volatile operator applied to a local bound to a different reference constructor, which throws ClassCastException",
    // The repair is either the sibling operator or the other constructor, and
    // which one is right depends on what coordination the code needs. This
    // package ships no fixes; see the README.
    Fixability::ReportOnly,
);

/// `examine_reference_bindings` only ever matches these seven heads.
///
/// The same seven names as `domain::REFERENCE_BINDING_HEADS`, in the form the
/// dispatcher's index is keyed by. The package's `engine_pass_tests` is what
/// pins the two together.
///
/// `let` is the most common head in the language, which is affordable only
/// because the body is never walked until a reference constructor has been
/// found in the binding vector — a delimiter test plus one `list_head` per
/// init, allocating nothing. That ordering is measured in the README.
const HEADS: [NormalizedHead; 7] = [
    NormalizedHead::new("if-let"),
    NormalizedHead::new("if-some"),
    NormalizedHead::new("let"),
    NormalizedHead::new("let*"),
    NormalizedHead::new("loop"),
    NormalizedHead::new("when-let"),
    NormalizedHead::new("when-some"),
];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    /// Common Lisp has `let` and `let*` too, and this rule's vocabulary would
    /// be nonsense there — which is what [`Rule::dialect_scope`] prevents, and
    /// why that override is the load-bearing declaration here rather than the
    /// head list.
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::CLOJURE_ONLY
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut reference_binding_count = 0;
        let mut items = Vec::new();
        examine_reference_bindings(view, &mut reference_binding_count, &mut items);
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
