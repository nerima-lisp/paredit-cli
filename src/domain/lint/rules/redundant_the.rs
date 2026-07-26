//! `redundant-the`: a (the t form) type declaration, which is vacuous and is just form (t matches every object).
//!
//! The analysis lives in [`crate::domain::redundant_the_report`], which also backs the
//! standalone `inspect redundant-the` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::redundant_the_report::{TheRedundancy, examine_the};
use crate::domain::semantics::NodeKey;
use crate::domain::semantics::typing::Ty;
use crate::domain::sexpr::ExpressionView;
use crate::domain::sexpr::reader::atom_symbol_text;

pub const META: RuleMeta = RuleMeta::new(
    "redundant-the",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a (the t form) type declaration, which is vacuous and is just form (t matches every object)",
    Fixability::Fixable,
);

/// `examine_the` only ever matches a `the` head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("the")];

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

        // Whether the form already provably has the type the `the` asserts,
        // which makes the assertion add nothing.
        //
        // An unmodelled or compound specifier (`fixnum`, `(integer 0 9)`)
        // yields `None` from `Ty::from_name` and the answer is no — this
        // layer declines to name those types, and a `the` it cannot read is
        // not one it may delete.
        //
        // `Bottom` is excluded because it is a subtype of everything, so a
        // form whose declarations contradict would otherwise satisfy every
        // assertion. That is dead code, not a redundant declaration.
        let already_known = |value_type: &ExpressionView, form: &ExpressionView| {
            let Some(asserted) = atom_symbol_text(value_type).and_then(Ty::from_name) else {
                return false;
            };
            NodeKey::of(form).is_some_and(|key| {
                let ty = context.type_table().expression_type(key);
                ty.is_definitely(asserted) && !ty.is_definitely(Ty::Bottom)
            })
        };

        let mut the_form_count = 0;
        let mut items = Vec::new();
        examine_the(
            view,
            context.path(),
            &already_known,
            &mut the_form_count,
            &mut items,
        );
        for item in items {
            let span = item.span;
            let (message, hint) = match &item.redundancy {
                TheRedundancy::Vacuous => (
                    "(the t form) is a vacuous type declaration; it is just form".to_string(),
                    "Drop the vacuous (the t …) declaration".to_string(),
                ),
                TheRedundancy::AlreadySatisfied(name) => (
                    format!(
                        "(the {name} form) asserts a type the form already has; it is just form"
                    ),
                    format!("Drop the (the {name} …) declaration the form already satisfies"),
                ),
            };

            // (the TYPE form) is form: replace the whole declaration with the
            // inner form.
            let fix = RuleFix::single(item.span, context_slice(item.form_span), hint);

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
    fn still_flags_the_vacuous_t_with_its_own_message() {
        assert_eq!(
            findings("(the t x)"),
            ["(the t form) is a vacuous type declaration; it is just form"]
        );
    }

    #[test]
    fn now_flags_a_standard_function_that_already_returns_the_asserted_type() {
        assert_eq!(
            findings("(the integer (length xs))"),
            ["(the integer form) asserts a type the form already has; it is just form"]
        );
    }

    #[test]
    fn now_flags_an_assertion_a_supertype_of_what_the_form_returns() {
        // `length` returns an integer, and every integer is a number, so
        // asserting `number` adds nothing either. The lattice is what makes
        // the weaker claim redundant too.
        assert_eq!(findings("(the number (length xs))").len(), 1);
    }

    #[test]
    fn now_flags_a_type_reached_through_a_binding() {
        assert_eq!(findings("(let ((n 5)) (the integer n))").len(), 1);
    }

    #[test]
    fn the_reasoning_does_not_run_through_the_assertion_being_judged() {
        // The trap this rule could fall into: if the type layer recorded a
        // `the` form's assertion against the *inner* form, every `the` would
        // look redundant. It records it against the `the` form itself, so an
        // inner form the layer cannot settle stays unsettled.
        assert!(findings("(the integer x)").is_empty());
        assert!(findings("(the integer (compute x))").is_empty());
    }

    #[test]
    fn a_form_of_unknown_type_is_not_flagged() {
        // `Unknown` maps to silence.
        assert!(findings("(the string (car xs))").is_empty());
        assert!(findings("(the list (gethash k h))").is_empty());
    }

    #[test]
    fn an_assertion_that_narrows_is_not_flagged() {
        // `length` returns an integer; `float` is a *different* type and the
        // assertion is a real (if wrong) claim, not a redundant one.
        assert!(findings("(the float (length xs))").is_empty());
        // `character` is narrower than nothing the form proves.
        assert!(findings("(the character (car xs))").is_empty());
    }

    #[test]
    fn an_unmodelled_or_compound_specifier_is_left_alone() {
        // A `the` this layer cannot read is not one it may delete.
        assert!(findings("(the fixnum (length xs))").is_empty());
        assert!(findings("(the (integer 0 9) (length xs))").is_empty());
        assert!(findings("(the my-struct (length xs))").is_empty());
    }

    #[test]
    fn a_non_common_lisp_dialect_is_not_flagged() {
        assert!(findings_for("(the integer (length xs))", Dialect::Clojure).is_empty());
        assert!(findings_for("(the integer (length xs))", Dialect::EmacsLisp).is_empty());
    }
}
