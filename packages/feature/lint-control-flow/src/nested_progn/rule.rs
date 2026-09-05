//! `nested-progn`: a multi-form progn nested directly inside another progn (its forms splice in).
//!

use paredit_core_lint_engine::LintResult;

use crate::nested_progn::domain::examine_progn;
use crate::support::is_hard_quoted_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "nested-progn",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a multi-form progn nested directly inside another progn (its forms splice in)",
    Fixability::Fixable,
);

/// `examine_progn` only inspects a `progn` head's own children.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("progn")];

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
        let mut progn_form_count = 0;
        let mut items = Vec::new();
        examine_progn(view, &mut progn_form_count, &mut items);
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
                // Splice the inner progn's body (exact source) in place of the
                // whole `(progn …)` wrapper.

                RuleFix::single(
                    item.span,
                    context_slice(item.body_span),
                    "Splice the nested progn into the enclosing progn".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                format!(
                    "progn with {} forms is nested directly in another progn; splice its forms in",
                    item.body_form_count
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

    fn count(source: &str) -> usize {
        run_rule_fixed(&ENTRIES, source).len()
    }

    #[test]
    fn still_fires_on_an_ordinary_nested_progn() {
        assert_eq!(
            fixed("(progn (a) (progn (b) (c)) (d))\n"),
            vec!["(progn (a) (b) (c) (d))\n".to_owned()]
        );
    }

    /// A template is code: the inner progn really is nested inside the outer
    /// one the macro emits, and splicing it is the right rewrite. Suppressing
    /// the whole quasiquote would be a false negative.
    #[test]
    fn still_fires_inside_a_quasiquote_template() {
        assert_eq!(
            fixed("(defmacro m () `(progn (a) (progn (b) (c))))\n"),
            vec!["(defmacro m () `(progn (a) (b) (c)))\n".to_owned()]
        );
    }

    /// The measured corruption: a `,@`-spliced child is not a nested progn but
    /// a form that runs at expansion time and contributes its *value*. Splicing
    /// it turned two expansion-time calls into emitted forms and dropped the
    /// `,@` with the wrapper, so the file stopped reading.
    #[test]
    fn a_spliced_child_progn_is_not_a_nested_progn() {
        assert_eq!(count("(defmacro m () `(progn (a) ,@(progn (b) (c))))\n"), 0);
    }

    #[test]
    fn a_quoted_child_progn_is_data() {
        assert_eq!(count("(progn (a) '(progn (b) (c)))\n"), 0);
    }

    /// Symmetric control: the prefix test is on the *child*, so an ordinary
    /// unprefixed child under a prefixed parent is still fixed — proving the
    /// guard did not widen to "anything under a prefix".
    #[test]
    fn an_unprefixed_child_under_a_prefixed_parent_still_fires() {
        assert_eq!(
            fixed("(defmacro m (x) `(progn ,x (progn (b) (c))))\n"),
            vec!["(defmacro m (x) `(progn ,x (b) (c)))\n".to_owned()]
        );
    }
}
