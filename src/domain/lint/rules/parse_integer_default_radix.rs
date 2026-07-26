//! `parse-integer-default-radix`: a parse-integer call with an explicit :radix 10, the default ((parse-integer s :radix 10) is (parse-integer s)).
//!
//! The analysis lives in [`crate::domain::parse_integer_default_radix_report`], which also backs the
//! standalone `inspect parse-integer-default-radix` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::NormalizedHead;
use crate::domain::lint::model::{
    Fixability, HeadFilter, Replacement, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::parse_integer_default_radix_report::examine;
use crate::domain::semantics::value::evaluate_constant;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "parse-integer-default-radix",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a parse-integer call with an explicit :radix 10, the default ((parse-integer s :radix 10) is (parse-integer s))",
    Fixability::Fixable,
);

/// The only head `examine` accepts.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("parse-integer")];

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
    ) -> Result<()> {
        // `#xA` and a constant bound to ten are the same redundant argument as
        // a literal `10`; `Unknown` reads as "not provably ten", so a radix
        // computed at run time is left alone.
        let is_ten = |radix: &ExpressionView| {
            evaluate_constant(
                context.dialect(),
                radix,
                context.binding_table(),
                context.value_table(),
            )
            .is_integer(10)
        };

        let mut call_form_count = 0;
        let mut items = Vec::new();
        examine(
            view,
            context.path(),
            &is_ten,
            &mut call_form_count,
            &mut items,
        );
        for item in items {
            let span = item.span;
            let fix = {
                let item = item.clone();

                RuleFix::multi(
                    "Drop the redundant :radix 10".to_owned(),
                    Replacement::new(item.removal_span, String::new()),
                    [],
                )
            };

            sink.report_fixed(
                span,
                "explicit :radix 10 restates parse-integer's default; drop it".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::dialect::Dialect;
    use crate::domain::lint_report::collect_lint_findings;
    use crate::domain::sexpr::SyntaxTree;
    use std::path::Path;

    fn hits(input: &str) -> usize {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        collect_lint_findings(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect")
            .into_iter()
            .filter(|finding| finding.rule == "parse-integer-default-radix")
            .count()
    }

    #[test]
    fn still_flags_the_literal_spelling() {
        assert_eq!(hits("(parse-integer s :radix 10)"), 1);
    }

    #[test]
    fn now_flags_ten_written_in_another_radix() {
        assert_eq!(hits("(parse-integer s :radix #xA)"), 1);
    }

    #[test]
    fn now_flags_ten_reached_through_a_binding() {
        assert_eq!(hits("(let ((r 10)) (parse-integer s :radix r))"), 1);
    }

    #[test]
    fn a_meaningful_radix_is_still_left_alone() {
        assert_eq!(hits("(parse-integer s :radix 16)"), 0);
        assert_eq!(hits("(parse-integer s :radix #x10)"), 0);
    }

    #[test]
    fn a_radix_computed_at_run_time_is_left_alone() {
        assert_eq!(hits("(parse-integer s :radix (current-radix))"), 0);
        assert_eq!(
            hits("(let ((r 10)) (setq r 16) (parse-integer s :radix r))"),
            0
        );
    }
}
