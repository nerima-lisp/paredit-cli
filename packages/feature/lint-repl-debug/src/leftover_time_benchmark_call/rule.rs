//! `leftover-time-benchmark-call`: a Common Lisp (time form) wrapper left in committed source.
//!
//! The analysis lives in [`crate::leftover_time_benchmark_call::domain`], which also backs the
//! standalone `inspect leftover-time-benchmark-call` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::leftover_time_benchmark_call::domain::examine;
use crate::support::{OperatorScope, evaluated_candidates};
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{Fixability, HeadFilter, RuleCategory, RuleMeta, Severity};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

/// Report-only, deliberately — the sibling demotion to `leftover-step-call`,
/// and for both of that rule's reasons at once.
///
/// The module doc on [`crate::leftover_time_benchmark_call::domain`] is correct
/// that CLHS `time` returns exactly `form`'s values, so unwrapping preserves the
/// *value* in every position. It cannot establish the two things that decide
/// whether unwrapping is right: that the head is `CL:TIME`, and that the timing
/// report is not what the file exists to produce. #129's principle applies
/// unchanged — the analysis proves the rewrite cannot change the body's value,
/// and a benchmark is valueless by construction, which is what makes it a
/// benchmark.
///
/// Measured over 31,634 SHA-256-deduplicated third-party files (1.108 GB) this
/// rule produced 248 findings, *every one carrying a fix*, across 53 files. On a
/// stratified sample of 50 (seed 20260805, strata = span-size tercile), **40
/// were false positives — 80%**, in two classes:
///
/// - **The timing report is the deliverable** (34 of the 40). The findings
///   cluster in files named for the fact: `coi/records/fast/timetest.lsp`,
///   `data-structures/memories/timetest.lsp`, `3bz/bench.lisp`,
///   `local-time/test/benchmarks.lisp`, `unicode/test-performance/tests.lsp`,
///   `sbcl/benchmarks/rwlbench.lisp`, `sicl/Papers/Generic-dispatch/benchmark.lisp`.
///   Unwrapping deletes the only output such a file has. One of them,
///   `sicl/…/test.lisp`, reads `(time (g4)) ; 3.3s` — the comment is the
///   author recording the number the rewrite would remove.
/// - **A binding form the walk does not model** (6 of the 40), where the
///   *variable name* is deleted and the code stops working:
///   `(with-foreign-object (time 'timespec) …)` becomes
///   `(with-foreign-object 'timespec …)`; `(dolist (time times) …)` becomes
///   `(dolist times …)`; ACL2's `(define add-vcd-chgs ((time stringp) …))`
///   becomes `(define add-vcd-chgs (stringp …))`; and in a `defthm` hint
///   `(:instance thm (time '0) (m …))` — a free-variable substitution alist —
///   `(time '0)` collapses to `'0`, corrupting the hint.
///   `support::binding_position` covers `let` and a `defun` lambda list; it does
///   not cover `dolist`, a CFFI binder, an ACL2 `define` parameter list, or an
///   `:instance` substitution.
///
/// The byte-delta oracle that caught #138 is blind here for the same reason as
/// `leftover-step-call`: 2,180 bytes over 53 files, **zero files cut below
/// half**, every file still parsing. An unwrap costs ~6 bytes.
///
/// Narrowing was measured and is unavailable: the head is a single symbol
/// (`time`), the shape is already pinned to exactly `(time form)`, and the
/// dialect scope is already Common-Lisp-only.
///
/// [`crate::support::OperatorScope`]: crate::support::OperatorScope
pub const META: RuleMeta = RuleMeta::new(
    "leftover-time-benchmark-call",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a Common Lisp (time form) wrapper left in committed source",
    Fixability::ReportOnly,
);

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::WholeTree
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
            // `form_span` stays computed because the standalone
            // `inspect leftover-time-benchmark-call` report still names the form
            // a human would keep — it just no longer becomes a rewrite the tool
            // applies on its own.
            sink.report(
                item.span,
                "time is a leftover benchmarking wrapper".to_owned(),
            );
        }
        Ok(())
    }
}
