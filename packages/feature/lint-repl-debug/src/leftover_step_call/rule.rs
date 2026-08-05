//! `leftover-step-call`: a Common Lisp (step form) wrapper left in committed source.
//!
//! The analysis lives in [`crate::leftover_step_call::domain`], which also backs the
//! standalone `inspect leftover-step-call` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::leftover_step_call::domain::examine;
use crate::support::{OperatorScope, evaluated_candidates};
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{Fixability, HeadFilter, RuleCategory, RuleMeta, Severity};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

/// Report-only, deliberately — the third demotion in this package, after
/// `leftover-print-debug` (#129) and `leftover-inspect-call` (#138), and on the
/// worst numbers of the three. [`crate::leftover_step_call::domain`] still
/// computes a `form_span`, and that span still points at exactly the bytes a
/// human would keep; the span is not the problem, what wraps it is.
///
/// The module doc on [`crate::leftover_step_call::domain`] is correct that CLHS
/// `step` returns exactly `form`'s values, so unwrapping is value-preserving in
/// every *position*. That argument assumes the head is `CL:STEP`. Establishing
/// that is what [`crate::support::OperatorScope`] cannot do: it resolves a
/// *lexical* table (`flet`/`labels`/`macrolet`), and `step` is overwhelmingly a
/// *package-level* operator in real Common Lisp — the state-transition function
/// of every machine model ever written.
///
/// Measured over 31,634 SHA-256-deduplicated third-party files (32,321 raw,
/// 1.108 GB) this rule produced 138 findings, *every one carrying a fix*, across
/// 59 files. On a stratified sample of 50 (seed 20260805, strata = span-size
/// tercile), **50 were false positives — 100%**, in six distinct classes:
///
/// - **A package-level `step` function.** `(equal s (step s))` becomes
///   `(equal s s)`. In ACL2's `books/models/jvm/m1/m1.lisp` the same run also
///   turns `(run (cdr sched) (step s))` into `(run (cdr sched) s)` — the machine
///   never advances — and rewrites the *statement* of `defthm step-opener`.
///   123 of the 138 findings are ACL2 machine models (M1, M2, TJVM, WyoM1, LL2,
///   WASM, ARM).
/// - **A binding form the walk does not model.** `(dolist (time times) …)` and
///   `(dolist (step xs) …)` lose the loop variable outright:
///   `(dolist times …)`. `support::binding_position` covers `let` and a `defun`
///   lambda list, not `dolist`.
/// - **An ACL2 `define` parameter list.** `(define f ((step svex-env-p)) …)`
///   becomes `(define f (svex-env-p) …)` — the parameter *name* is deleted and
///   its guard silently becomes the name.
/// - **An ACL2 `b*` binder.** `(b* ((step (strings-to-symbol …))) …)` loses the
///   bound variable the same way.
/// - **`useless-runes` data files**, where `(M1::STEP (1 1 (:TYPE-PRESCRIPTION
///   M1::STEP)))` is rune metadata, not a call, and the head is package
///   qualified besides.
/// - **SBCL's own `tests/step.pure.lisp`**, the test suite *for* `CL:STEP`,
///   where `(step (fib 3))` is the subject under test.
///
/// Note what the byte-delta oracle that caught #138 reports here: 1,073 bytes
/// over 59 files and **zero files cut below half**. An unwrap costs ~7 bytes, so
/// the metric that made #138 obvious is blind to this rule. Every rewritten file
/// still parses, so `read-before-not-after` is 0 as well. The damage is
/// semantic, and only adjudication finds it.
///
/// Narrowing was measured and is unavailable: the head is a single symbol
/// (`step`), so there is no head list to trim, and the dialect scope is already
/// Common-Lisp-only. Suppressing the package-level case would require a
/// cross-file definition table this rule does not have.
///
/// [`crate::support::OperatorScope`]: crate::support::OperatorScope
pub const META: RuleMeta = RuleMeta::new(
    "leftover-step-call",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a Common Lisp (step form) wrapper left in committed source",
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
            // `inspect leftover-step-call` report still names the form a human
            // would keep — it just no longer becomes a rewrite the tool applies
            // on its own.
            sink.report(item.span, "step is a leftover stepping wrapper".to_owned());
        }
        Ok(())
    }
}
