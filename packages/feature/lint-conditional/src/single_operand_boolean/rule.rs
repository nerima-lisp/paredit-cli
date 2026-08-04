//! `single-operand-boolean`: a single-operand and/or ((and X) and (or X) are just X).
//!
//! The analysis lives in [`crate::single_operand_boolean::domain`], which also backs the
//! standalone `inspect single-operand-boolean` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::single_operand_boolean::domain::examine_boolean;
use crate::support::is_hard_quoted_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "single-operand-boolean",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a single-operand and/or ((and X) and (or X) are just X)",
    Fixability::Fixable,
);

/// The two heads `examine_boolean` accepts.
const HEADS: [NormalizedHead; 2] = [NormalizedHead::new("and"), NormalizedHead::new("or")];

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
        let mut boolean_form_count = 0;
        let mut items = Vec::new();
        examine_boolean(view, &mut boolean_form_count, &mut items);
        for item in items {
            let span = item.span;
            // A hard-quoted form is a literal list, not code: rewriting its
            // contents edits the program's data rather than its behaviour.
            // `'(or x)` is a two-element list and `'x` is a symbol, so the
            // "fix" changes what the datum *is*.
            //
            // The verdict is the `hard` counter alone. A `` `(…) `` template's
            // contents really are emitted as code, so suppressing there would
            // go quiet on the macro bodies this rule exists to read.
            //
            // Asked only here, once a finding already exists — never per
            // visited node — so a file with no findings never pays for it.
            if is_hard_quoted_at(context.tree(), span) {
                continue;
            }
            let fix = {
                // Replace the wrapper with its sole operand, copied verbatim.

                // The fix region is `content_span`, not `span`: `span` starts at this
                // form's *own* reader prefixes, so replacing it deletes them. A
                // `` `(…) `` has to keep its backquote — without it the commas
                // underneath are commas outside a backquote, and the file stops
                // reading altogether. The two spans coincide on any form with no
                // prefix, which is almost all code, so nothing else moves.
                RuleFix::single(
                    view.content_span,
                    context_slice(item.inner_span),
                    format!("Unwrap the single-operand {}", item.operator),
                )
            };

            sink.report_fixed(
                span,
                format!(
                    "{} has a single operand; ({} X) is just X",
                    item.operator, item.operator
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

    // ----- positive controls: the rule still does its job ------------------

    #[test]
    fn still_fires_on_an_ordinary_unquoted_single_operand_or() {
        assert_eq!(
            fixed("(defun f (x) (or x))\n"),
            vec!["(defun f (x) x)\n".to_owned()]
        );
    }

    #[test]
    fn still_fires_on_and() {
        assert_eq!(
            fixed("(defun f (x) (and (g x)))\n"),
            vec!["(defun f (x) (g x))\n".to_owned()]
        );
    }

    /// The control that matters most for the span change: a template really is
    /// code, and collapsing the `or` inside one is the right rewrite. A guard
    /// that went quiet on every quasiquote would pass every other test here.
    #[test]
    fn still_fires_inside_a_quasiquote_template() {
        assert_eq!(
            fixed("(defmacro m (x) `(when c (or ,x)))\n"),
            vec!["(defmacro m (x) `(when c ,x))\n".to_owned()]
        );
    }

    // ----- the corruption this rule shipped with ---------------------------

    /// The measured defect: the fix replaced `view.span`, which begins at the
    /// backquote, so `` `(or ,x) `` became `,x` — a comma outside any
    /// backquote, which SBCL refuses to read.
    #[test]
    fn a_quasiquoted_single_operand_or_keeps_its_backquote() {
        assert_eq!(
            fixed("(defmacro m (x) `(or ,x))\n"),
            vec!["(defmacro m (x) `,x)\n".to_owned()]
        );
    }

    /// `'(or x)` is a two-element *list*; `'x` is a symbol. Preserving the
    /// quote was never enough — the rewrite changed what the datum is, and the
    /// guard now declines to report it at all.
    #[test]
    fn a_hard_quoted_single_operand_or_is_not_a_finding_at_all() {
        assert_eq!(count("(defparameter *f* '(or x))\n"), 0);
    }

    /// The measured shape: a quoted *expected value* in a test table. Rewriting
    /// `'(and x)` to `'x` makes `(and x)` expand to itself and the assertion
    /// vacuous.
    #[test]
    fn a_boolean_inside_a_quoted_ancestor_is_not_a_finding() {
        assert_eq!(count("(deftest e (check '(\"single\" (and x) x)))\n"), 0);
    }

    #[test]
    fn a_long_hand_quoted_single_operand_or_is_not_a_finding() {
        assert_eq!(count("(defparameter *f* (quote (or x)))\n"), 0);
    }

    /// `` `(or ,@predicates) `` is not a one-operand `or`: `predicates` may hold
    /// none or several. The old fix wrote `` `,@predicates ``, which SBCL
    /// rejects outright — five corpus files stopped reading on exactly this.
    #[test]
    fn a_spliced_operand_is_not_one_operand() {
        assert_eq!(count("(defmacro m (ps) `(or ,@ps))\n"), 0);
        assert_eq!(count("(defmacro m (ps) `(and ,@ps))\n"), 0);
    }

    /// The symmetric negative control for the splice guard: a plain unquote is
    /// exactly one operand and must still be fixed, or the guard has quietly
    /// widened to "anything with a comma in it".
    #[test]
    fn a_plain_unquoted_operand_is_still_one_operand() {
        assert_eq!(
            fixed("(defmacro m (p) `(or ,p))\n"),
            vec!["(defmacro m (p) `,p)\n".to_owned()]
        );
    }

    // ----- unchanged behaviour ---------------------------------------------

    #[test]
    fn two_operands_and_a_reader_conditional_are_left_alone() {
        assert_eq!(count("(defun f (x y) (or x y))\n"), 0);
        assert_eq!(count("(defun f () (or #+sbcl x))\n"), 0);
        assert_eq!(count(""), 0);
    }
}
