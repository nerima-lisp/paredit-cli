//! `leftover-trace-call`: `trace`/`untrace` used as a statement, left in committed source.
//!

use paredit_core_lint_engine::LintResult;

use crate::leftover_trace_call::domain::examine;
use crate::support::{OperatorScope, evaluated_candidates};
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{Fixability, HeadFilter, RuleCategory, RuleMeta, Severity};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::ExpressionView;

/// Report-only, deliberately. Unlike its siblings this rule's position analysis
/// works well — it withholds a fix from 269 of 288 corpus findings — so the
/// demotion rests on the 19 it does offer, all of which were adjudicated.
///
/// Measured over 31,634 SHA-256-deduplicated third-party files (1.108 GB): 288
/// findings across 55 files, 19 carrying a fix, 785 bytes deleted. Those 19 were
/// adjudicated as a **census, not a sample — 19 of 19 were false positives,
/// 100%**, in four classes:
///
/// - **A test suite for `TRACE` itself.** 11 of the 19 are SBCL's own
///   `tests/trace.impure.lisp`, `tests/interface.impure.lisp`,
///   `tests/debug.impure.lisp` and `tests/eval.impure.lisp`, which call `trace`
///   and then *assert on the trace output*:
///   `(assert (search "0: (TRACED-GF 3)" output))`. Deleting the `(trace …)`
///   makes the assertion fail. `contrib/sb-introspect/xref-test-data.lisp` is
///   the same shape as fixture data.
/// - **A package-qualified head that is not `CL:TRACE`.** Radiance's
///   `(l:trace :core.request "Executing request: ~s ~s" request response)` is a
///   *logging* call at trace level. The head match ignores the package
///   qualifier entirely, so `l:trace`, `log4cl:trace` and any other library
///   spelling are all matched as if they were `CL:TRACE`.
/// - **The implementation of a tracing feature**, e.g. SLY's
///   `contrib/slynk-trace-dialog.lisp`.
/// - **Demonstration code**, e.g. Norvig's PAIP `krep.lisp`, where
///   `(trace index)` … `(untrace index)` brackets the behaviour being
///   demonstrated; deleting only the `trace` leaves an orphan `untrace`.
///
/// One of the 19 also exposes a span defect worth recording: deleting
/// `(untrace test:function)` from SBCL's `tests/package-locks.impure.lisp`
/// replaces **337 bytes for a 23-byte span**, because
/// [`crate::support::removal_span`] absorbs backward to the previous sibling
/// when the form is last in its group — swallowing five lines of comment that
/// document the *following* form.
///
/// Narrowing was measured and rejected. Dropping `untrace` from
/// [`crate::leftover_trace_call::domain::HEADS`] leaves 13 fixable findings, all
/// still false positives, so it removes no defect. Gating to one dialect is
/// unavailable: every one of the 19 is Common Lisp, the stratum a narrowing
/// would have to keep.
///
/// [`crate::support::removal_span`]: crate::support::removal_span
pub const META: RuleMeta = RuleMeta::new(
    "leftover-trace-call",
    RuleCategory::Suspicious,
    Severity::Warning,
    "trace or untrace used as a statement, left in committed source",
    Fixability::ReportOnly,
);

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::WholeTree
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::new(&[Dialect::CommonLisp, Dialect::EmacsLisp])
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let candidates = evaluated_candidates(context, view);
        let mut items = Vec::new();
        let scope = OperatorScope::shared(context);
        examine(candidates, &scope, context.path(), &mut items);
        for item in items {
            // No `report_fixed` branch, deliberately: see `META`. `item`'s
            // `fix_span` stays computed because the standalone
            // `inspect leftover-trace-call` report still names the span a human
            // would delete — it just no longer becomes a rewrite the tool
            // applies on its own.
            sink.report(
                item.span,
                format!("{} is a leftover debugging statement", item.head),
            );
        }
        Ok(())
    }
}
