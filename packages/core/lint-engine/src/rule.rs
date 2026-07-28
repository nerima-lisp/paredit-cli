//! The contract every lint rule implements.

use std::fmt;

use paredit_core_syntax::sexpr::ExpressionView;

use super::engine::{RuleContext, RuleSink};
use super::error::LintResult;
use super::model::{HeadFilter, RuleCategory, RuleMeta, Severity};
use super::policy::RuleDialectScope;

/// One lint rule.
///
/// A rule declares *which* nodes it wants ([`LintRule::head_filter`]) and
/// *what* it does with one ([`LintRule::check`]); it never walks the document
/// itself. That inversion is what lets 130+ rules share a single pass, and it
/// is the seam the semantic layers plug into — a rule that needs the binding or
/// value table reads it from the [`RuleContext`] instead of rebuilding it.
///
/// Rule *metadata* deliberately lives on [`RuleEntry`] rather
/// than on this trait: the public `RULES`, `RULE_DOCS`, `FIXABLE_RULES`, and
/// `WARNING_RULES` constants are derived from the registry at compile time, and
/// a trait method cannot be called in a `const` context.
pub trait LintRule: Sync + fmt::Debug {
    /// Which nodes of the single pass this rule wants to see.
    fn head_filter(&self) -> HeadFilter;

    /// The dialects the rule is meaningful for. Almost every rule encodes CLHS
    /// operator semantics, so Common Lisp only is the default.
    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::COMMON_LISP_ONLY
    }

    /// Examines one matched node and reports what it finds.
    ///
    /// Fallible because the handful of rules that still consult the whole tree
    /// resolve paths, and a resolution failure must surface rather than be
    /// silently read as "no findings". The handful is four; [`LintError`] names
    /// the only thing they can fail with.
    ///
    /// [`LintError`]: crate::error::LintError
    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult;
}

/// One registered rule: its compile-time description and its behaviour.
///
/// The metadata is a plain field rather than a trait method because a trait
/// method cannot be called in a `const` context, and the catalogue constants
/// must be derivable at compile time.
#[derive(Debug)]
pub struct RuleEntry {
    meta: &'static RuleMeta,
    rule: &'static (dyn LintRule + Sync),
}

impl RuleEntry {
    #[must_use]
    pub const fn new(meta: &'static RuleMeta, rule: &'static (dyn LintRule + Sync)) -> Self {
        Self { meta, rule }
    }

    #[must_use]
    pub const fn meta(&self) -> &'static RuleMeta {
        self.meta
    }

    #[must_use]
    pub const fn rule(&self) -> &'static (dyn LintRule + Sync) {
        self.rule
    }
}

/// The rule set one engine run operates over.
///
/// The engine and the policy functions take this instead of reaching for a
/// `REGISTRY` constant. That inversion is what lets the engine live in its own
/// package: section 4.2 requires the registry to sit outside the engine crate,
/// because a registry naming every rule while every rule depends on the engine
/// is a cycle. Passing the catalogue in leaves the engine ignorant of which
/// rules exist, and of how many.
///
/// Everything the catalogue publishes is derived from the entries, so a
/// derived view cannot drift from the list it came from.
#[derive(Debug, Clone, Copy)]
pub struct RuleCatalog {
    entries: &'static [RuleEntry],
}

impl RuleCatalog {
    #[must_use]
    pub const fn new(entries: &'static [RuleEntry]) -> Self {
        Self { entries }
    }

    #[must_use]
    pub const fn entries(&self) -> &'static [RuleEntry] {
        self.entries
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.entries.len()
    }

    /// Always false for the shipped catalogue, but required of a public `len`
    /// and meaningful for a catalogue built in a test.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Rule names in registration order, which is also report order.
    pub fn names(&self) -> impl Iterator<Item = &'static str> + use<'_> {
        self.entries
            .iter()
            .map(|entry| entry.meta().name().as_str())
    }

    /// The category names `--category` accepts. Independent of the entries:
    /// the categories are a closed set in the model.
    #[must_use]
    pub fn categories(&self) -> Vec<&'static str> {
        RuleCategory::ALL
            .iter()
            .map(|category| category.as_str())
            .collect()
    }

    fn meta_of(&self, name: &str) -> Option<&'static RuleMeta> {
        self.entries
            .iter()
            .map(RuleEntry::meta)
            .find(|meta| meta.name().as_str() == name)
    }

    #[must_use]
    pub fn category_of(&self, name: &str) -> Option<RuleCategory> {
        self.meta_of(name).map(RuleMeta::category)
    }

    /// `Error` for an unknown name, matching the historical `contains`-based
    /// behaviour the catalogue replaced.
    #[must_use]
    pub fn severity_of(&self, name: &str) -> Severity {
        self.meta_of(name)
            .map_or(Severity::Error, RuleMeta::severity)
    }
}
