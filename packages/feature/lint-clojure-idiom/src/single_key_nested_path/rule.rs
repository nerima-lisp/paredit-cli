//! `single-key-nested-path`: a nested-path accessor that reaches through
//! nothing.
//!
//! The analysis lives in [`crate::single_key_nested_path::domain`], which also
//! backs the standalone `inspect single-key-nested-path` command; this module
//! only registers it with the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::single_key_nested_path::domain::examine_nested_path;
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
    "single-key-nested-path",
    // The path vector is built and destructured for a traversal of length one.
    RuleCategory::Allocation,
    // Identical behaviour, so this is legibility and a small allocation, not a
    // bug: `(get-in m [:k])` does what it says, just the long way round.
    Severity::Warning,
    "an assoc-in/update-in/get-in whose path holds one key, which the direct operator says",
    // The rewrite is mechanical and provably identity-preserving — see the
    // `core.clj` derivation in `domain` — but this package ships no fixes; see
    // the README.
    Fixability::ReportOnly,
);

/// `examine_nested_path` only ever matches these three heads.
///
/// The same three names as `domain::NESTED_PATH_HEADS`, in the form the
/// dispatcher's index is keyed by. The package's `engine_pass_tests` is what
/// pins the two together: a head spelled only here would leave every domain
/// test green while the rule never received a node.
const HEADS: [NormalizedHead; 3] = [
    NormalizedHead::new("assoc-in"),
    NormalizedHead::new("update-in"),
    NormalizedHead::new("get-in"),
];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    /// The trait default is Common Lisp only, which has none of these three
    /// operators and cannot read a `[…]` path at all.
    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::CLOJURE_ONLY
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut nested_path_count = 0;
        let mut items = Vec::new();
        examine_nested_path(view, &mut nested_path_count, &mut items);
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
