//! `zero-divisor`: a division-family form with a literal 0 divisor, a guaranteed division-by-zero ((/ x 0)).
//!
//! The analysis lives in [`crate::domain::zero_divisor_report`], which also backs the
//! standalone `inspect zero-divisor` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::semantics::value::evaluate_constant;
use crate::domain::sexpr::ExpressionView;
use crate::domain::zero_divisor_report::examine;

pub const META: RuleMeta = RuleMeta::new(
    "zero-divisor",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a division-family form with a literal 0 divisor, a guaranteed division-by-zero ((/ x 0))",
    Fixability::ReportOnly,
);

/// Every operator `examine` accepts: `/` plus the quotient-family ops.
const HEADS: [NormalizedHead; 11] = [
    NormalizedHead::new("/"),
    NormalizedHead::new("mod"),
    NormalizedHead::new("rem"),
    NormalizedHead::new("floor"),
    NormalizedHead::new("ceiling"),
    NormalizedHead::new("truncate"),
    NormalizedHead::new("round"),
    NormalizedHead::new("ffloor"),
    NormalizedHead::new("fceiling"),
    NormalizedHead::new("ftruncate"),
    NormalizedHead::new("fround"),
];

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
        // The value layer sees through a binding and through folded
        // arithmetic, so `(let ((z 0)) (/ x z))` and `(/ x (- 1 1))` are the
        // same bug as `(/ x 0)`. `Unknown` reads as "not provably zero", which
        // is what keeps this from firing on code that merely might divide by
        // zero at run time.
        let is_zero = |divisor: &ExpressionView| {
            evaluate_constant(
                context.dialect(),
                divisor,
                context.binding_table(),
                context.value_table(),
            )
            .is_integer(0)
        };

        let mut division_form_count = 0;
        let mut items = Vec::new();
        examine(
            view,
            context.path(),
            &is_zero,
            &mut division_form_count,
            &mut items,
        );
        for item in items {
            let span = item.span;

            sink.report(
                span,
                format!(
                    "{} by a literal 0 always signals division-by-zero",
                    item.operator
                ),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::dialect::Dialect;
    use crate::domain::lint::policy::RuleSelection;
    use crate::domain::lint_report::collect_lint_findings;
    use crate::domain::sexpr::SyntaxTree;
    use std::path::Path;

    fn findings(input: &str) -> Vec<String> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        collect_lint_findings(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect")
            .into_iter()
            .filter(|finding| finding.rule == "zero-divisor")
            .map(|finding| finding.message)
            .collect()
    }

    #[test]
    fn still_flags_the_literal_spelling() {
        assert_eq!(findings("(/ x 0)").len(), 1);
        assert_eq!(findings("(mod x 0)").len(), 1);
    }

    #[test]
    fn now_flags_a_zero_reached_through_a_binding() {
        // The case the value layer exists for: the reader sees `z`, not `0`.
        assert_eq!(findings("(let ((z 0)) (/ x z))").len(), 1);
    }

    #[test]
    fn now_flags_a_zero_reached_through_folded_arithmetic() {
        assert_eq!(findings("(/ x (- 1 1))").len(), 1);
    }

    #[test]
    fn now_flags_a_zero_written_in_another_radix() {
        assert_eq!(findings("(/ x #x0)").len(), 1);
    }

    #[test]
    fn now_flags_a_zero_reached_through_a_file_constant() {
        assert_eq!(findings("(defconstant +none+ 0)(/ x +none+)").len(), 1);
    }

    #[test]
    fn a_binding_that_might_be_reassigned_is_not_flagged() {
        // Zero false positives is the whole contract: the value at the use
        // site is whatever the `setq` wrote.
        assert!(findings("(let ((z 0)) (setq z 1) (/ x z))").is_empty());
    }

    #[test]
    fn a_divisor_that_is_merely_unknown_is_not_flagged() {
        assert!(findings("(/ x y)").is_empty());
        assert!(findings("(/ x (read))").is_empty());
        assert!(findings("(let ((z (read))) (/ x z))").is_empty());
    }

    #[test]
    fn a_float_zero_is_still_left_alone() {
        // `0.0` is a different, implementation-defined story, as before.
        assert!(findings("(/ x 0.0)").is_empty());
    }

    #[test]
    fn a_zero_numerator_is_still_fine() {
        assert!(findings("(let ((z 0)) (/ z x))").is_empty());
    }

    #[test]
    fn the_rule_is_still_selectable_by_name() {
        assert!(RuleSelection::Only(&["zero-divisor"]).includes(META.name()));
        assert!(!RuleSelection::Only(&["if-not"]).includes(META.name()));
    }
}
