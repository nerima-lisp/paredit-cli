//! `format-to-string`: a (format nil "~A"/"~S" x), which is (princ-to-string x)/(prin1-to-string x).
//!
//! The analysis lives in [`crate::format_to_string::domain`], which also backs the
//! standalone `inspect format-to-string` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::format_to_string::domain::examine;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "format-to-string",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a (format nil \"~A\"/\"~S\" x), which is (princ-to-string x)/(prin1-to-string x)",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("format")];

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
        let mut format_form_count = 0;
        let mut items = Vec::new();
        examine(view, &mut format_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                // (format nil "~A"/"~S" x) is (princ-to-string x)/(prin1-to-string x).
                let text = format!(
                    "({} {})",
                    item.replacement,
                    context_slice(item.argument_span)
                );

                // The fix region is `content_span`, not `span`: `span` starts at this
                // form's *own* reader prefixes, so replacing it deletes them. A
                // `` `(…) `` has to keep its backquote — without it the commas
                // underneath are commas outside a backquote, and the file stops
                // reading altogether. The two spans coincide on any form with no
                // prefix, which is almost all code, so nothing else moves.
                RuleFix::single(
                    view.content_span,
                    text,
                    format!("Rewrite (format nil … x) as ({} x)", item.replacement),
                )
            };

            sink.report_fixed(
                span,
                format!(
                    "format to a string is just {}; use ({} x)",
                    item.replacement, item.replacement
                ),
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
    fn still_fires_on_an_ordinary_unquoted_format() {
        assert_eq!(
            fixed("(defun f (i) (format nil \"~A\" i))\n"),
            vec!["(defun f (i) (princ-to-string i))\n".to_owned()]
        );
    }

    #[test]
    fn a_quasiquoted_format_keeps_its_backquote() {
        assert_eq!(
            fixed("(defmacro m (i) `(format nil \"~A\" ,i))\n"),
            vec!["(defmacro m (i) `(princ-to-string ,i))\n".to_owned()]
        );
    }
}
