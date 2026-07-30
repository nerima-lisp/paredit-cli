//! `make-hash-table-test`: a make-hash-table with an explicit :test 'eql, the default ((make-hash-table :test 'eql) is (make-hash-table)).
//!
//! The analysis lives in [`crate::make_hash_table_test::domain`], which also backs the
//! standalone `inspect make-hash-table-test` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::make_hash_table_test::domain::examine;
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
        _context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut make_hash_table_form_count = 0;
        let mut items = Vec::new();
        examine(view, &mut make_hash_table_form_count, &mut items);
        for item in items {
            let span = item.span;
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
