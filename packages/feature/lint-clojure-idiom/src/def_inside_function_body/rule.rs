//! `def-inside-function-body`: a namespace Var interned at call time.
//!
//! The analysis lives in [`crate::def_inside_function_body::domain`], which
//! also backs the standalone `inspect def-inside-function-body` command; this
//! module only registers it with the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::def_inside_function_body::domain::examine_function_body;
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
    "def-inside-function-body",
    // Well-formed code whose meaning is almost certainly not what was meant:
    // the author wrote what looks like a local and got a namespace global.
    RuleCategory::Suspicious,
    // The Var does not exist until the function runs, and concurrent callers
    // race on it. That is a defect, not a style preference — which is also
    // clj-kondo's judgement, where `:inline-def` is on by default.
    Severity::Error,
    "a def/defn/defonce inside a function body, which interns a namespace Var at call time",
    // `let` is usually right, a top-level `defonce` sometimes is, and which one
    // depends on whether the value is meant to outlive the call. That is the
    // author's decision.
    Fixability::ReportOnly,
);

/// `examine_function_body` only ever matches these three heads.
///
/// The *definers*, not `fn`: anchoring on `fn` as well would report one inner
/// `def` once per enclosing closure, and would run the body scan on every
/// closure in the file rather than on every named function. `defmacro` is
/// absent because a macro emitting a `def` is how definers are written.
const HEADS: [NormalizedHead; 3] = [
    NormalizedHead::new("defn"),
    NormalizedHead::new("defn-"),
    NormalizedHead::new("defmethod"),
];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    /// The trait default is Common Lisp only, where `def` is not an operator
    /// and `defvar` inside a `defun` is a different question.
    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::CLOJURE_ONLY
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut function_count = 0;
        let mut items = Vec::new();
        examine_function_body(view, &mut function_count, &mut items);
        if items.is_empty() {
            return Ok(());
        }
        // Only once a candidate exists. `examine_function_body` has already
        // declined a `def` that is quoted *within* an evaluated function; this
        // is the other half — an entire `(defn …)` that is itself data, which
        // the dispatcher hands over like any other head match.
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        for item in items {
            sink.report(item.span, item.message());
        }
        Ok(())
    }
}
