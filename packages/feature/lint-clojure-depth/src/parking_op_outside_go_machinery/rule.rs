//! `parking-op-outside-go-machinery`: a parking channel op the `go` transform
//! never rewrote.
//!

use paredit_core_lint_engine::LintResult;

use crate::parking_op_outside_go_machinery::domain::examine_go_block_parking;
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
    "parking-op-outside-go-machinery",
    // The defect exists only because of the IOC threading model: the same
    // expression one level out is correct.
    RuleCategory::Concurrency,
    // `<!` outside the transform is `(assert nil "<! used not in (go ...)
    // block")` — a runtime failure on the first execution, or a silent `nil`
    // when `*assert*` was false at compile time.
    Severity::Error,
    "a core.async parking op (<!, >!, alts!, alt!) inside a go body but behind a function or thread boundary the go transform does not rewrite",
    // The repair is a restructuring — `doseq` instead of `for`, a `go` inside
    // the `thread`, an explicit loop — and the choice is the author's.
    Fixability::ReportOnly,
);

/// `examine_go_block_parking` only ever matches these two heads.
///
/// The same two names as `domain::GO_BLOCK_HEADS`, in the form the
/// dispatcher's index is keyed by. The package's `engine_pass_tests` is what
/// pins the two together.
const HEADS: [NormalizedHead; 2] = [NormalizedHead::new("go"), NormalizedHead::new("go-loop")];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    /// The trait default is Common Lisp only, which has no `core.async`.
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
        examine_go_block_parking(view, &mut go_block_count, &mut items);
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
