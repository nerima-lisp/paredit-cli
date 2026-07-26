//! `char-op-string`: a character function (char=/char-code/alpha-char-p/...) applied to a string literal (type error).
//!
//! The analysis lives in [`crate::domain::char_op_string_report`], which also backs the
//! standalone `inspect char-op-string` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::char_op_string_report::{CharacterMismatch, examine_call};
use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::semantics::NodeKey;
use crate::domain::semantics::typing::Ty;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "char-op-string",
    RuleCategory::Malformed,
    Severity::Error,
    "a character function (char=/char-code/alpha-char-p/...) applied to a string literal (type error)",
    Fixability::ReportOnly,
);

/// Every function `examine_call` recognizes as requiring character arguments.
const HEADS: [NormalizedHead; 25] = [
    NormalizedHead::new("char="),
    NormalizedHead::new("char/="),
    NormalizedHead::new("char<"),
    NormalizedHead::new("char>"),
    NormalizedHead::new("char<="),
    NormalizedHead::new("char>="),
    NormalizedHead::new("char-equal"),
    NormalizedHead::new("char-not-equal"),
    NormalizedHead::new("char-lessp"),
    NormalizedHead::new("char-greaterp"),
    NormalizedHead::new("char-not-lessp"),
    NormalizedHead::new("char-not-greaterp"),
    NormalizedHead::new("char-code"),
    NormalizedHead::new("char-int"),
    NormalizedHead::new("char-upcase"),
    NormalizedHead::new("char-downcase"),
    NormalizedHead::new("char-name"),
    NormalizedHead::new("digit-char-p"),
    NormalizedHead::new("alpha-char-p"),
    NormalizedHead::new("alphanumericp"),
    NormalizedHead::new("upper-case-p"),
    NormalizedHead::new("lower-case-p"),
    NormalizedHead::new("both-case-p"),
    NormalizedHead::new("graphic-char-p"),
    NormalizedHead::new("standard-char-p"),
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
        // The type context turns "spelled as a string" into "cannot be a
        // character". The question is asked in the negative on purpose:
        // `is_definitely(Ty::String)` would miss `(char= (length xs) c)`,
        // while `is_definitely_not(Ty::Character)` catches every settled type
        // that shares no member with `character` and still says nothing about
        // an argument the layer could not settle.
        //
        // `Bottom` is excluded because it shares no member with anything, so a
        // binding whose declarations contradict would otherwise answer
        // "definitely not a character" — that is dead code, not a type error
        // at this call.
        let is_non_character = |argument: &ExpressionView| {
            NodeKey::of(argument).is_some_and(|key| {
                let ty = context.type_table().expression_type(key);
                ty.is_definitely_not(Ty::Character) && !ty.is_definitely(Ty::Bottom)
            })
        };

        let mut char_call_count = 0;
        let mut items = Vec::new();
        examine_call(
            view,
            context.path(),
            &is_non_character,
            &mut char_call_count,
            &mut items,
        );
        for item in items {
            let span = item.span;

            let message = match &item.mismatch {
                CharacterMismatch::StringLiteral(literal) => format!(
                    "{} is given string literal {literal}; it requires a character (type error)",
                    item.operator
                ),
                CharacterMismatch::InferredType => format!(
                    "{} is given an argument of an inferred non-character type; it requires a character (type error)",
                    item.operator
                ),
            };

            sink.report(span, message);
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
            findings(r#"(char= "a" c)"#),
            [r#"char= is given string literal "a"; it requires a character (type error)"#]
        );
    }

    #[test]
    fn a_literal_still_wins_over_the_inferred_type() {
        // `(length xs)` is not a character either, but the reader can see the
        // `"a"`, so the message keeps naming it.
        assert_eq!(
            findings(r#"(char= (length xs) "a")"#),
            [r#"char= is given string literal "a"; it requires a character (type error)"#]
        );
    }

    #[test]
    fn now_flags_a_standard_function_whose_return_type_is_not_a_character() {
        assert_eq!(
            findings("(char= (length xs) c)"),
            [
                "char= is given an argument of an inferred non-character type; it requires a character (type error)"
            ]
        );
    }

    #[test]
    fn now_flags_a_non_character_reached_through_a_binding() {
        assert_eq!(findings("(let ((n 5)) (char= n c))").len(), 1);
    }

    #[test]
    fn now_flags_a_declared_non_character() {
        assert_eq!(findings("(char-code (the integer x))").len(), 1);
    }

    #[test]
    fn now_flags_a_string_that_is_not_written_as_a_literal() {
        // The case the spelling test was always reaching for, finally stated
        // as the type it is.
        assert_eq!(findings("(char-upcase (symbol-name s))").len(), 1);
    }

    #[test]
    fn an_argument_of_unknown_type_is_not_flagged() {
        // `Unknown` must map to silence: this is the whole discipline the
        // type layer is worth having.
        assert!(findings("(char= a b)").is_empty());
        assert!(findings("(char= (compute x) c)").is_empty());
        assert!(findings("(char-code (car xs))").is_empty());
    }

    #[test]
    fn an_argument_that_really_is_a_character_is_not_flagged() {
        // The type context answers here; the answer is `character`.
        assert!(findings("(char= (char-upcase c) d)").is_empty());
        assert!(findings("(char= (code-char n) d)").is_empty());
        assert!(findings(r"(char= #\a c)").is_empty());
    }

    #[test]
    fn a_non_common_lisp_dialect_is_not_flagged() {
        // The type layer is CLHS-only, and so is the report's dialect gate.
        assert!(findings_for("(char= (length xs) c)", Dialect::Clojure).is_empty());
        assert!(findings_for("(char= (length xs) c)", Dialect::EmacsLisp).is_empty());
    }
}
