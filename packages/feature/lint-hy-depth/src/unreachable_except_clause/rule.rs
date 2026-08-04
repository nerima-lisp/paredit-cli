//! `hy-unreachable-except-clause`: an `except` clause an earlier one shadows.
//!
//! Hy's `try` compiles to a Python `Try` node, and Python runs the **first**
//! handler whose type the exception is an instance of. A clause on a supertype
//! therefore makes every later clause on one of its subtypes dead, and nothing
//! about the form says so — the narrow handler, which is the one somebody
//! thought about, never runs.
//!
//! This is `pylint`'s `E0701 bad-except-order` and `W0705 duplicate-except`,
//! read through Hy's surface syntax. Neither Hy nor CPython rejects the shape:
//! it compiles, it runs, and the dead branch is simply never taken.
//!
//! The analysis lives in [`crate::unreachable_except_clause::domain`]; this
//! module registers it and phrases its findings.
//!
//! # Why this is not the sibling package's `hy-bare-except`
//!
//! That rule anchors on `except` and asks whether *one* clause is too broad.
//! This one anchors on `try`, because reachability is a property of a clause's
//! position among its siblings and a rule handed one clause at a time cannot
//! see it. The two report different spans: `hy-bare-except` reports the bare
//! clause, this reports the clauses the bare clause kills.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::support::{hy_head, is_unevaluated_at};
use crate::unreachable_except_clause::domain::{DIALECTS, Shadow, examine_try};

pub const META: RuleMeta = RuleMeta::new(
    "hy-unreachable-except-clause",
    RuleCategory::Conditions,
    Severity::Error,
    "an except clause whose exception type an earlier clause in the same try already catches",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "Python runs the first `except` handler whose type the raised exception is an instance \
         of, and Hy's `try` compiles to exactly that. A clause naming a supertype — or naming the \
         same type twice, or a bare `(except [] …)` — therefore shadows every later clause it \
         covers, which then never runs for any input. `pylint` reports the same two shapes as \
         `E0701` and `W0705`.",
    )
    .with_example(
        "(try\n  (parse text)\n  (except [e Exception]\n    (log e))\n  (except [e ValueError]\n \
         \x20  (retry)))",
        "(try\n  (parse text)\n  (except [e ValueError]\n    (retry))\n  (except [e Exception]\n \
         \x20  (log e)))",
    )
    .with_caveat(
        "Only Python's own builtin exception hierarchy is known here. A clause on a project's \
         own exception class is never called shadowed by another named type, because this layer \
         cannot see what it inherits from — the single exception is `BaseException` and the bare \
         `(except [] …)`, which catch every exception there is. In particular an earlier \
         `Exception` is *not* treated as covering a user-defined class, which may derive from \
         `BaseException` directly.",
    ),
);

/// Exactly the heads the domain can match, pinned to `domain::HEADS` by the
/// test below.
const FILTER_HEADS: [NormalizedHead; 1] = [NormalizedHead::new("try")];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&FILTER_HEADS)
    }

    /// Reads `domain::DIALECTS`, so the dialect gate and this scope cannot
    /// drift apart. The trait's default is `COMMON_LISP_ONLY`, and a rule that
    /// omits this override silently never runs on Hy at all.
    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::new(&DIALECTS)
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        // Defence in depth, and known to be redundant *today*: the index keys
        // non-Common-Lisp heads verbatim, so `(TRY …)` never arrives, and it
        // does not offer `#(try …)` — mutation testing with this comparison
        // deleted failed no test in this package. It stays because the index
        // documents itself as a pre-filter that may be *wider* than a rule's
        // notion of the operator, so a rule that leaned on it for what a `try`
        // is would be correct only by accident of the dispatcher's shape. The
        // sibling Hy package keeps the same guard for the same reason.
        if hy_head(view) != Some("try") {
            return Ok(());
        }
        let dead = examine_try(view);
        if dead.is_empty() {
            return Ok(());
        }
        // Only now, with findings otherwise ready: this one descends from the
        // top-level form and is four orders of magnitude dearer than the shape
        // checks above.
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        for clause in dead {
            let detail = match &clause.reason {
                Shadow::CatchAll => {
                    format!("clause {} catches every exception", clause.shadowed_by)
                }
                Shadow::SameType(name) => {
                    format!("clause {} already names `{name}`", clause.shadowed_by)
                }
                Shadow::Supertype(name) => format!(
                    "clause {} names `{name}`, which this one's type inherits from",
                    clause.shadowed_by
                ),
            };
            sink.report(
                clause.span,
                format!(
                    "this except clause is unreachable: {detail}, so Python's first-match \
                     handler selection never reaches clause {}; move the narrower clause above \
                     the broader one",
                    clause.position
                ),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unreachable_except_clause::domain::HEADS;

    /// The `HeadFilter` is a pre-filter: over-approximating only wastes a call,
    /// but under-approximating silently drops every finding at the missing
    /// head. Pinning the two makes adding a head to one and not the other a
    /// test failure rather than a silent hole.
    #[test]
    fn the_filter_heads_are_exactly_the_domain_heads() {
        let filter: Vec<&str> = FILTER_HEADS.iter().map(|head| head.as_str()).collect();
        assert_eq!(filter, HEADS.to_vec());
    }

    /// `NormalizedHead::new` asserts at compile time, but the domain's own
    /// spellings are plain `&str` and could drift into a shape the head index
    /// can never produce.
    #[test]
    fn every_domain_head_is_index_normalized() {
        for head in HEADS {
            assert!(!head.is_empty());
            assert_eq!(head, head.to_ascii_lowercase(), "{head} is not normalized");
        }
    }
}
