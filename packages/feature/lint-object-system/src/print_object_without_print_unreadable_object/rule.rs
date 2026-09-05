//! `print-object-without-print-unreadable-object`: a `print-object` method that
//! writes to the stream directly.
//!

use paredit_core_lint_engine::LintResult;

use crate::print_object_without_print_unreadable_object::domain::examine_print_object_without_print_unreadable_object;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "print-object-without-print-unreadable-object",
    // `ObjectSystem`: the subject is conformance to a CLOS printing protocol,
    // not the formatting of the string itself.
    RuleCategory::ObjectSystem,
    // A warning: a method deliberately emitting a re-readable external
    // representation is exactly what `*print-readably*` is for, and this rule
    // cannot tell that apart from an accident.
    Severity::Warning,
    "a print-object method that writes to the stream directly",
    // No fix. Wrapping the body in `print-unreadable-object` changes what the
    // method emits — the `#<…>` framing is added text — so the rewrite cannot
    // claim to preserve the form's value.
    Fixability::ReportOnly,
);

/// `examine_print_object_without_print_unreadable_object` only ever matches a
/// `defmethod` head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("defmethod")];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut print_object_method_count = 0;
        let mut items = Vec::new();
        examine_print_object_without_print_unreadable_object(
            context.tree(),
            view,
            &mut print_object_method_count,
            &mut items,
        );
        for item in items {
            sink.report(
                item.span,
                format!(
                    "this print-object method writes with {} instead of \
                     print-unreadable-object: its output ignores *print-readably* rather than \
                     signalling print-not-readable",
                    item.writer
                ),
            );
        }
        Ok(())
    }
}
