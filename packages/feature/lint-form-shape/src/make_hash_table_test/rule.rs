//! `make-hash-table-test`: a make-hash-table with an explicit :test 'eql, the default ((make-hash-table :test 'eql) is (make-hash-table)).
//!

use paredit_core_lint_engine::LintResult;

use crate::make_hash_table_test::domain::examine;
use crate::support::is_hard_quoted_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, Replacement, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "make-hash-table-test",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a make-hash-table with an explicit :test 'eql, the default ((make-hash-table :test 'eql) is (make-hash-table))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("make-hash-table")];

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
        let mut make_hash_table_form_count = 0;
        let mut items = Vec::new();
        examine(view, &mut make_hash_table_form_count, &mut items);
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
                // Delete the redundant ` :test 'eql` argument pair.

                RuleFix::multi(
                    "Drop the redundant :test 'eql".to_owned(),
                    Replacement::new(item.removal_span, String::new()),
                    [],
                )
            };

            sink.report_fixed(
                span,
                "the make-hash-table :test defaults to eql; drop the explicit :test 'eql"
                    .to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
