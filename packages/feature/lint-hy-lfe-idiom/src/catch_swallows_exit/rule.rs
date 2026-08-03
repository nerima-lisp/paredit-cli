//! `lfe-catch-swallows-exit`: `(catch Expr)`, whose result cannot be told
//! apart from a value the expression legitimately returned.
//!
//! Erlang's `catch` BIF turns an exit or an error into the term
//! `{'EXIT', Reason}` and a throw into the thrown value — and hands both back
//! as though they were ordinary results. The caller then has no way to tell a
//! failure from a success, because `{'EXIT', Reason}` is also a perfectly
//! ordinary tuple a function may return.
//!
//! That is not a theoretical objection. Measured, LFE 2.2.0 on Erlang
//! 27.3.4.15:
//!
//! ```text
//! (defun risky-exit ()   (exit 'boom))
//! (defun honest-tuple () (tuple 'EXIT 'boom))
//!
//! (catch (risky-exit))    =>  {'EXIT',boom}
//! (honest-tuple)          =>  {'EXIT',boom}
//! (catch (honest-tuple))  =>  {'EXIT',boom}
//! ```
//!
//! All three are the same term. A `catch` around the second one reports a
//! failure that never happened, and no amount of care at the call site can
//! recover the difference. `(catch (throw 'oops))` is worse still: it returns
//! the bare atom `oops`, which is indistinguishable from a normal return.
//!
//! `try … catch` does not have this problem — it separates the success and
//! failure continuations rather than encoding both in one term — and it also
//! keeps the stacktrace, which `catch` discards.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, RuleTag,
    Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::catch_swallows_exit::domain::{DIALECTS, is_clause_of};
use crate::shared::{list_head, node_context};

pub const META: RuleMeta = RuleMeta::new(
    "lfe-catch-swallows-exit",
    RuleCategory::Conditions,
    Severity::Warning,
    "`(catch Expr)` returns failures as ordinary terms, indistinguishable from success",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "Erlang's `catch` BIF encodes failure into its return value: an exit or error becomes \
         `{'EXIT', Reason}` and a throw becomes the thrown term, both returned as though nothing \
         went wrong. Since `{'EXIT', Reason}` is also an ordinary tuple a function may return, \
         the caller cannot tell the two apart. Measured on LFE 2.2.0, `(catch (exit 'boom))` and \
         a plain `(tuple 'EXIT 'boom)` produce the identical term. `try … catch` separates the \
         success and failure paths instead, and keeps the stacktrace that `catch` discards.",
    )
    .with_example(
        "(catch (do-work Args))",
        "(try\n  (do-work Args)\n  (catch\n    ((tuple type reason stack)\n     (log-failure type reason stack))))",
    )
    .with_caveat(
        "The `catch` *clause* of a `try` is a different form that happens to share the symbol — \
         `(try Expr (catch …))` is the shape this rule recommends — and is never reported.",
    ),
)
// Tagged `pedantic`, so only the `pedantic` preset includes it.
//
// The mechanism is real and was measured, but an audit over 2604 third-party
// `.lfe` files produced 146 findings and a good share of them are deliberate.
// Of the 146: 28 are the subject of an enclosing `case`, where the author is
// explicitly discriminating `#(EXIT …)` from success; 28 more are in statement
// position with the value discarded, which is best-effort telemetry
// (`(catch (prometheus_counter:inc …))`) or optional startup
// (`(catch (mnesia:start))`). That is the tag's definition: correct, and noise
// on a codebase that has not adopted the convention.
//
// What kept the rule rather than killing it is the *spread*. Findings appear
// in 9 of the corpus's ~143 repositories — 94% are clean — and two of the
// three heaviest are LFE's own implementation. This is not a rule that fires
// on everything; it fires on the handful of projects that use `catch` as their
// house error-handling style.
.with_tags(&[RuleTag::Pedantic]);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("catch")];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::new(&DIALECTS)
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        if list_head(view) != Some("catch") {
            return Ok(());
        }
        // `(catch)` on its own is not the expression form.
        if view.children.len() < 2 {
            return Ok(());
        }
        // Only now, with a finding otherwise ready, the single root-view
        // descent. It materializes the whole document, so doing it before the
        // head and arity checks would cost the file's size on every visited
        // node; asking for the parent head and the quote state separately
        // would cost it twice.
        let enclosing = node_context(context.tree(), view.span);
        if is_clause_of(enclosing.parent_head.as_deref()) {
            return Ok(());
        }
        if enclosing.is_data {
            return Ok(());
        }
        sink.report(
            view.span,
            "`catch` returns a failure as an ordinary term — `{'EXIT', Reason}` for an exit, the \
             thrown term for a throw — which the caller cannot tell from a value the expression \
             legitimately returned; use `try … catch`"
                .to_owned(),
        );
        Ok(())
    }
}
