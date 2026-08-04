//! `cons-to-list`: a cons onto nil or a list literal ((cons a nil) is (list a); (cons a (list b)) is (list a b)).
//!
//! The analysis lives in [`crate::cons_to_list::domain`], which also backs the
//! standalone `inspect cons-to-list` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::cons_to_list::domain::examine_cons;
use crate::support::is_hard_quoted_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "cons-to-list",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a cons onto nil or a list literal ((cons a nil) is (list a); (cons a (list b)) is (list a b))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("cons")];

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
        let context_slice = |span| context.slice(span).to_owned();
        let mut cons_form_count = 0;
        let mut items = Vec::new();
        examine_cons(view, &mut cons_form_count, &mut items);
        for item in items {
            let span = item.span;
            // Rewriting hard-quoted data edits a user's data literal rather than
            // code, and no round-trip property catches it. Read on the `hard`
            // counter alone: a `` `(…) `` template's contents really are emitted as
            // code. See `support::is_hard_quoted_at`.
            if is_hard_quoted_at(context.tree(), span) {
                continue;
            }
            let fix = {
                // Rewrite as `(list ELEMENT [TAIL_ELEMENTS])`.
                let element = context_slice(item.element_span);
                let text = match item.tail_elements_span {
                    Some(tail) => format!("(list {} {})", element, context_slice(tail)),
                    None => format!("(list {element})"),
                };

                // The fix region is `content_span`, not `span`: `span` starts at this
                // form's *own* reader prefixes, so replacing it deletes them. A
                // `` `(…) `` has to keep its backquote — without it the commas
                // underneath are commas outside a backquote, and the file stops
                // reading altogether. The two spans coincide on any form with no
                // prefix, which is almost all code, so nothing else moves.
                RuleFix::single(
                    view.content_span,
                    text,
                    "Rewrite the cons as a list".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "cons onto nil/a list is a list constructor; use list".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::run_rule_fixed;
    use paredit_core_lint_engine::rule::RuleEntry;

    static ENTRIES: [RuleEntry; 1] = [RuleEntry::new(&META, &RULE)];

    /// The source each finding's fix produces, in report order.
    fn fixed(source: &str) -> Vec<String> {
        run_rule_fixed(&ENTRIES, source)
            .into_iter()
            .map(|(_, source)| source)
            .collect()
    }

    #[allow(dead_code)]
    fn count(source: &str) -> usize {
        run_rule_fixed(&ENTRIES, source).len()
    }

    #[test]
    fn still_fires_on_an_ordinary_unquoted_cons() {
        assert_eq!(
            fixed("(defun f (x) (cons x nil))\n"),
            vec!["(defun f (x) (list x))\n".to_owned()]
        );
    }

    /// Two defects, one after the other, and the second is why this no longer
    /// asserts a rewrite at all.
    ///
    /// First the fix replaced `view.span`, which begins at the quote, so
    /// `'(cons x nil)` became `(list x)` — the quote gone and a literal list
    /// turned into a call. Moving the fix to `content_span` fixed *that* and
    /// left `'(list x)`, which is still wrong: `'(cons x nil)` is a
    /// three-element list of symbols, and `'(list x)` is a two-element one, so
    /// the rewrite silently edits a user's data literal either way. The rule
    /// now declines to fix inside a hard quote, so the datum is left alone.
    #[test]
    fn a_quoted_cons_is_data_and_is_left_alone() {
        assert_eq!(count("(defparameter *p* '(cons x nil))\n"), 0);
    }

    /// A template is code, so the rewrite is right here and must still happen.
    #[test]
    fn still_fires_inside_a_quasiquote_template() {
        assert_eq!(
            fixed("(defmacro m (x) `(f (cons ,x nil)))\n"),
            vec!["(defmacro m (x) `(f (list ,x)))\n".to_owned()]
        );
    }

    #[test]
    fn a_quasiquoted_cons_keeps_its_backquote() {
        assert_eq!(
            fixed("(defmacro m (x) `(cons ,x nil))\n"),
            vec!["(defmacro m (x) `(list ,x))\n".to_owned()]
        );
    }
}
