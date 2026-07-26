//! `eq-char-comparison`: an eq compared against a character literal (eq on characters is unreliable; use eql/char=).
//!
//! The analysis lives in [`crate::domain::eq_char_comparison_report`], which also backs the
//! standalone `inspect eq-char-comparison` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::eq_char_comparison_report::{CharacterEvidence, examine_comparison};
use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::semantics::NodeKey;
use crate::domain::semantics::typing::Ty;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "eq-char-comparison",
    RuleCategory::Suspicious,
    Severity::Error,
    "an eq compared against a character literal (eq on characters is unreliable; use eql/char=)",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("eq")];

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
        // The type context turns "spelled as a character" into "is a
        // character". `Unknown` says nothing, which is what keeps this silent
        // on an argument the layer could not settle; `Bottom` is excluded
        // because it is a subtype of everything, so a binding whose
        // declarations contradict would otherwise answer "definitely a
        // character" — that is dead code, not an `eq` bug.
        let is_character = |argument: &ExpressionView| {
            NodeKey::of(argument).is_some_and(|key| {
                let ty = context.type_table().expression_type(key);
                ty.is_definitely(Ty::Character) && !ty.is_definitely(Ty::Bottom)
            })
        };

        let mut comparison_form_count = 0;
        let mut items = Vec::new();
        examine_comparison(
            view,
            context.path(),
            &is_character,
            &mut comparison_form_count,
            &mut items,
        );
        for item in items {
            let span = item.span;
            let fix = {
                RuleFix::single(
                    item.head_span,
                    "eql".to_owned(),
                    "Compare with eql (eq is unreliable on characters)".to_owned(),
                )
            };

            let message = match &item.evidence {
                CharacterEvidence::Literal(literal) => {
                    format!("eq compares against character literal {literal}; use eql or char=")
                }
                CharacterEvidence::InferredType => {
                    "eq compares against an argument of inferred type character; use eql or char="
                        .to_owned()
                }
            };

            sink.report_fixed(span, message, fix);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::dialect::Dialect;
    use crate::domain::lint_report::collect_lint_findings;
    use crate::domain::sexpr::SyntaxTree;
    use std::path::Path;

    fn findings_for(input: &str, dialect: Dialect) -> Vec<String> {
        let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("parse");
        collect_lint_findings(Path::new("test.lisp"), dialect, &tree)
            .expect("collect")
            .into_iter()
            .filter(|finding| finding.rule == META.name().as_str())
            .map(|finding| finding.message)
            .collect()
    }

    fn findings(input: &str) -> Vec<String> {
        findings_for(input, Dialect::CommonLisp)
    }

    #[test]
    fn still_flags_the_literal_spelling_with_its_own_message() {
        assert_eq!(
            findings("(eq c #\\a)"),
            ["eq compares against character literal #\\a; use eql or char="]
        );
    }

    #[test]
    fn a_literal_still_wins_over_the_inferred_type() {
        assert_eq!(
            findings("(eq (char s 0) #\\a)"),
            ["eq compares against character literal #\\a; use eql or char="]
        );
    }

    #[test]
    fn now_flags_a_standard_function_with_a_character_return_type() {
        assert_eq!(
            findings("(eq (char s 0) c)"),
            ["eq compares against an argument of inferred type character; use eql or char="]
        );
        assert_eq!(findings("(eq (code-char n) c)").len(), 1);
    }

    #[test]
    fn now_flags_a_character_reached_through_a_binding() {
        assert_eq!(findings("(let ((c #\\a)) (eq c d))").len(), 1);
    }

    #[test]
    fn now_flags_a_declared_character() {
        assert_eq!(findings("(eq (the character x) y)").len(), 1);
    }

    #[test]
    fn an_argument_of_unknown_type_is_not_flagged() {
        assert!(findings("(eq x y)").is_empty());
        assert!(findings("(eq (compute x) y)").is_empty());
        assert!(findings("(eq (aref s 0) c)").is_empty());
    }

    #[test]
    fn an_argument_of_a_settled_non_character_type_is_not_flagged() {
        assert!(findings("(eq (length xs) n)").is_empty());
        assert!(findings("(eq (string-upcase s) x)").is_empty());
        assert!(findings("(eq (consp x) y)").is_empty());
    }

    #[test]
    fn a_non_common_lisp_dialect_is_not_flagged() {
        // The type layer is CLHS-only, and so is the report's dialect gate.
        assert!(findings_for("(eq (char s 0) c)", Dialect::Clojure).is_empty());
        assert!(findings_for("(eq (char s 0) c)", Dialect::EmacsLisp).is_empty());
    }

    #[test]
    fn a_quoted_form_the_type_context_cannot_settle_is_not_flagged() {
        // `'(char s 0)` denotes a list, not the call's result; the layer is
        // opaque to quoted forms and stays silent rather than guessing.
        assert!(findings("(eq '(char s 0) c)").is_empty());
    }
}
