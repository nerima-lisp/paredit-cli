//! `go-block-blocking-channel-op`: a `!!` channel operation on a go-block
//! thread.
//!
//! The analysis lives in
//! [`crate::go_block_blocking_channel_op::domain`], which also backs the
//! standalone `inspect go-block-blocking-channel-op` command; this module only
//! registers it with the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::go_block_blocking_channel_op::domain::examine_go_block;
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
    "go-block-blocking-channel-op",
    // Not merely "shared state": a depleted pool stops every go block in the
    // process, which is a defect that exists only because of the threading
    // model.
    RuleCategory::Concurrency,
    // core.async's own docstring: "risks depleting the fixed pool of go block
    // threads, causing all go block processing to stop". A deadlock that takes
    // the whole pool with it is not a style question.
    Severity::Error,
    "a core.async blocking op (<!!, >!!, alts!!, alt!!) reached from a go body, which can deplete the fixed go-block thread pool",
    // Deleting one `!` is the repair *most* of the time, and the rest of the
    // time it is `(thread …)` — a restructuring only the author can choose.
    // This package ships no fixes; see the README.
    Fixability::ReportOnly,
);

/// `examine_go_block` only ever matches these two heads.
///
/// The same two names as `domain::GO_BLOCK_HEADS`, in the form the
/// dispatcher's index is keyed by. The package's `engine_pass_tests` is what
/// pins the two together: a head spelled only here would leave every domain
/// test green while the rule never received a node.
const HEADS: [NormalizedHead; 2] = [NormalizedHead::new("go"), NormalizedHead::new("go-loop")];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    /// The trait default is Common Lisp only, which has no `go` macro in this
    /// sense and no `core.async` at all.
    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::CLOJURE_ONLY
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut go_block_count = 0;
        let mut items = Vec::new();
        examine_go_block(view, &mut go_block_count, &mut items);
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
