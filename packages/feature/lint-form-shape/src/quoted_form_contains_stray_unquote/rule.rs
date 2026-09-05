//! `quoted-form-contains-stray-unquote`: a `,`/`,@` inside a quoted form with no backquote above it.
//!

use paredit_core_lint_engine::LintResult;

use crate::quoted_form_contains_stray_unquote::domain::examine;
use crate::support::is_unevaluated_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "quoted-form-contains-stray-unquote",
    // The form's shape is not what the operator requires: a `quote`'s datum
    // carrying a reader escape the reader will refuse.
    RuleCategory::Malformed,
    // SBCL refuses to read the file at all — "Comma not inside a backquote" —
    // so this is not a style opinion.
    Severity::Error,
    "a quoted form containing a , or ,@ with no enclosing backquote, which Common Lisp refuses to read",
    Fixability::ReportOnly,
);

/// See the domain module: a `'`-prefixed list's head is its first element, so
/// `'(a ,b)` written outside all four of these is out of `Heads`'s reach and is
/// a documented false negative.
const HEADS: [NormalizedHead; 4] = [
    NormalizedHead::new("defmacro"),
    NormalizedHead::new("define-compiler-macro"),
    NormalizedHead::new("macrolet"),
    NormalizedHead::new("quote"),
];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    /// Cheapest predicate first: [`examine`]'s head comparison rejects
    /// everything the head index let through for another rule, and its subtree
    /// walk allocates only its own stack. The quote descent runs last and only
    /// once a stray comma has actually been found — which, on any file whose
    /// macros are written correctly, is never.
    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut quote_context_count = 0;
        let mut items = Vec::new();
        examine(view, &mut quote_context_count, &mut items);
        if items.is_empty() || is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        for item in items {
            // The comma itself, not the enclosing form: a `defmacro` can carry
            // several, and each one is its own edit.
            let span = item.unquote_span;
            let message = paredit_core_cli::report::Finding::message(&item);
            sink.report(span, message);
        }
        Ok(())
    }
}
