//! `constant-if-test`: an if whose test is the literal t or nil ((if t a b) is a; (if nil a b) is b).
//!
//! The analysis lives in [`crate::domain::constant_if_test_report`], which also backs the
//! standalone `inspect constant-if-test` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::constant_if_test_report::examine_if;
use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::NormalizedHead;
use crate::domain::lint::model::{
    Fixability, HeadFilter, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::semantics::value::evaluate_constant;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "constant-if-test",
    RuleCategory::DeadCode,
    Severity::Warning,
    "an if whose test is the literal t or nil ((if t a b) is a; (if nil a b) is b)",
    Fixability::Fixable,
);

/// The only head `examine_if` accepts.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("if")];

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
        let context_slice = |span| context.slice(span).to_owned();
        // A test the value layer settles is as decided as a literal `t`: the
        // branch not taken is dead either way. `Unknown` leaves the form
        // alone, which is what keeps a genuine run-time condition untouched.
        let constant_test = |test: &ExpressionView| {
            evaluate_constant(
                context.dialect(),
                test,
                context.binding_table(),
                context.value_table(),
            )
            .truthiness(context.dialect())
        };

        let mut if_form_count = 0;
        let mut items = Vec::new();
        examine_if(
            view,
            context.path(),
            &constant_test,
            &mut if_form_count,
            &mut items,
        );
        for item in items {
            let span = item.span;
            let fix = {
                let item = item.clone();
                // Replace the whole form with the live branch (or `nil` for a false
                // one-armed if), dropping the dead branch.
                let text = match item.result_span {
                    Some(span) => context_slice(span),
                    None => "nil".to_owned(),
                };

                RuleFix::single(
                    item.span,
                    text,
                    format!("Drop the dead branch of the constant {} test", item.test),
                )
            };

            sink.report_fixed(
                span,
                format!("if test is the constant {}; one branch is dead", item.test),
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

    fn messages(input: &str) -> Vec<String> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        collect_lint_findings(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect")
            .into_iter()
            .filter(|finding| finding.rule == "constant-if-test")
            .map(|finding| finding.message)
            .collect()
    }

    #[test]
    fn still_flags_the_literal_spellings() {
        assert_eq!(messages("(if t 1 2)").len(), 1);
        assert_eq!(messages("(if nil 1 2)").len(), 1);
    }

    #[test]
    fn now_flags_a_test_the_value_layer_settles() {
        assert_eq!(messages("(if (= 1 1) 1 2)").len(), 1);
        assert_eq!(messages("(if (zerop 0) 1 2)").len(), 1);
        assert_eq!(messages("(if (< 2 1) 1 2)").len(), 1);
    }

    #[test]
    fn now_flags_a_test_reached_through_a_binding() {
        assert_eq!(messages("(let ((flag t)) (if flag 1 2))").len(), 1);
    }

    #[test]
    fn the_message_still_names_the_settled_value() {
        // The wording frame is unchanged; a folded test reports the constant
        // it settles to rather than the source text it was written as.
        let message = messages("(if (= 1 1) 1 2)").pop().expect("one finding");
        assert!(message.contains('t'), "{message}");
    }

    #[test]
    fn a_genuine_run_time_condition_is_left_alone() {
        assert!(messages("(if x 1 2)").is_empty());
        assert!(messages("(if (ready-p) 1 2)").is_empty());
        assert!(messages("(let ((flag t)) (setq flag nil) (if flag 1 2))").is_empty());
    }

    #[test]
    fn a_truthy_non_boolean_constant_still_settles_the_branch() {
        // Every value but `nil` is true in Common Lisp, so `5` decides the
        // branch just as `t` does.
        assert_eq!(messages("(if 5 1 2)").len(), 1);
    }
}
