//! `empty-let`: a let with an empty binding list, which is just progn ((let () body) is (progn body)).
//!

use paredit_core_lint_engine::LintResult;

use crate::empty_let::domain::examine_let;
use crate::support::is_hard_quoted_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, Replacement, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "empty-let",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a let with an empty binding list, which is just progn ((let () body) is (progn body))",
    Fixability::Fixable,
);

/// `examine_let` only matches bare `let`, not `let*` (see the module doc).
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("let")];

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
        let mut let_form_count = 0;
        let mut items = Vec::new();
        examine_let(view, &mut let_form_count, &mut items);
        for item in items {
            let span = item.span;
            // A rewrite of a form inside `'(…)` or `(quote …)` edits a
            // *data literal*, not code, so the finding is dropped rather
            // than fixed. Read on the `hard` counter alone: a `` `(…) ``
            // template's contents really are emitted as code, and going
            // quiet there would abandon the macro bodies this rule exists
            // to read. Asked once per finding, never per visited node.
            if is_hard_quoted_at(context.tree(), span) {
                continue;
            }
            let fix = {
                // Replace the `(let ()` prefix with `(progn`, leaving the body intact.

                RuleFix::multi(
                    "Rewrite the empty let as progn".to_owned(),
                    Replacement::new(item.prefix_span, "(progn".to_owned()),
                    [],
                )
            };

            sink.report_fixed(
                span,
                "let with no bindings is just progn; (let () body) is (progn body)".to_owned(),
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
    fn still_fires_on_an_ordinary_unquoted_empty_let() {
        assert_eq!(
            fixed("(defun f () (let () (a) (b)))\n"),
            vec!["(defun f () (progn (a) (b)))\n".to_owned()]
        );
    }

    /// The measured defect: the rewritten region began at `view.span`, so the
    /// `(progn` written over `(let ()` landed on top of the backquote too.
    #[test]
    fn a_quasiquoted_empty_let_keeps_its_backquote() {
        assert_eq!(
            fixed("(defmacro m (x) `(let () ,x))\n"),
            vec!["(defmacro m (x) `(progn ,x))\n".to_owned()]
        );
    }

    #[test]
    fn still_fires_inside_a_quasiquote_template() {
        assert_eq!(
            fixed("(defmacro m (x) `(f (let () ,x)))\n"),
            vec!["(defmacro m (x) `(f (progn ,x)))\n".to_owned()]
        );
    }
}
