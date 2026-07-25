//! The contract every lint rule implements.

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::ExpressionView;

use super::engine::{RuleContext, RuleSink};
use super::model::HeadFilter;
use super::policy::RuleDialectScope;

/// One lint rule.
///
/// A rule declares *which* nodes it wants ([`LintRule::head_filter`]) and
/// *what* it does with one ([`LintRule::check`]); it never walks the document
/// itself. That inversion is what lets 130+ rules share a single pass, and it
/// is the seam the semantic layers plug into — a rule that needs the binding or
/// value table reads it from the [`RuleContext`] instead of rebuilding it.
///
/// Rule *metadata* deliberately lives on [`super::registry::RuleEntry`] rather
/// than on this trait: the public `RULES`, `RULE_DOCS`, `FIXABLE_RULES`, and
/// `WARNING_RULES` constants are derived from the registry at compile time, and
/// a trait method cannot be called in a `const` context.
pub trait LintRule: Sync {
    /// Which nodes of the single pass this rule wants to see.
    fn head_filter(&self) -> HeadFilter;

    /// The dialects the rule is meaningful for. Almost every rule encodes CLHS
    /// operator semantics, so Common Lisp only is the default.
    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::CommonLispOnly
    }

    /// Examines one matched node and reports what it finds.
    ///
    /// Fallible because the handful of rules that still consult the whole tree
    /// resolve paths, and a resolution failure must surface rather than be
    /// silently read as "no findings".
    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> Result<()>;
}

/// Whether this rule runs at all for `dialect`.
pub fn applies_to(rule: &dyn LintRule, dialect: Dialect) -> bool {
    rule.dialect_scope().includes(dialect)
}
