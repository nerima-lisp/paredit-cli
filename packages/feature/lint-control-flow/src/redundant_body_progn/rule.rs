//! `redundant-body-progn`: a multi-form progn used as a when/unless/let/defun/... body (its forms splice in).
//!

use paredit_core_lint_engine::LintResult;

use crate::redundant_body_progn::domain::examine_form;
use crate::support::is_hard_quoted_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "redundant-body-progn",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a multi-form progn used as a when/unless/let/defun/... body (its forms splice in)",
    Fixability::Fixable,
);

/// Every implicit-progn head `body_start` recognizes (`redundant_body_progn_report`).
const HEADS: [NormalizedHead; 14] = [
    NormalizedHead::new("when"),
    NormalizedHead::new("unless"),
    NormalizedHead::new("dolist"),
    NormalizedHead::new("dotimes"),
    NormalizedHead::new("block"),
    NormalizedHead::new("lambda"),
    NormalizedHead::new("let"),
    NormalizedHead::new("let*"),
    NormalizedHead::new("flet"),
    NormalizedHead::new("labels"),
    NormalizedHead::new("macrolet"),
    NormalizedHead::new("catch"),
    NormalizedHead::new("defun"),
    NormalizedHead::new("defmacro"),
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
    ) -> LintResult<()> {
        let context_slice = |span| context.slice(span).to_owned();
        let mut implicit_progn_form_count = 0;
        let mut items = Vec::new();
        examine_form(view, &mut implicit_progn_form_count, &mut items);
        // `'(when c (progn a b))` is a literal list whose third element happens
        // to read like a body progn; splicing it edits the program's data. The
        // enclosing form carries the quote here, not the progn, so the
        // prefix test in `is_progn` cannot see it.
        //
        // A quasiquote is deliberately not suppressed: in
        // `` `(when c (progn ,a ,b)) `` the progn really is a body form of the
        // `when` this macro emits, and splicing it is the right rewrite.
        //
        // Asked only once the walk above already produced a finding, so a file
        // of `when`/`let`/`defun` forms with no redundant progn in any of them
        // never descends anything.
        if !items.is_empty() && is_hard_quoted_at(context.tree(), view.span) {
            return Ok(());
        }
        for item in items {
            let span = item.span;
            let fix = {
                // Splice the progn's body (exact source) in place of the wrapper.

                RuleFix::single(
                    item.span,
                    context_slice(item.body_span),
                    format!("Splice the progn into the enclosing {}", item.parent),
                )
            };

            sink.report_fixed(
                span,
                format!(
                    "progn with {} forms is a {} body; splice its forms in",
                    item.body_form_count, item.parent
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
    fn still_fires_on_an_ordinary_unquoted_body_progn() {
        assert_eq!(
            fixed("(when c (progn (a) (b)))\n"),
            vec!["(when c (a) (b))\n".to_owned()]
        );
    }

    #[test]
    fn still_fires_in_a_defmacro_body() {
        // A bare progn directly in the macro's own body: the macro's body is an
        // implicit progn already, so this one really is redundant.
        assert_eq!(
            fixed("(defmacro m (x) (progn (check x) (build x)))\n"),
            vec!["(defmacro m (x) (check x) (build x))\n".to_owned()]
        );
    }

    #[test]
    fn still_fires_inside_a_quasiquote_template() {
        // The `when` is emitted code and the progn is its body form, so
        // splicing is right. Only the template's *own* backquote is off limits.
        assert_eq!(
            fixed("(defmacro m (a b) `(when c (progn ,a ,b)))\n"),
            vec!["(defmacro m (a b) `(when c ,a ,b))\n".to_owned()]
        );
    }

    #[test]
    fn still_fires_on_every_implicit_progn_head() {
        for source in [
            "(unless c (progn (a) (b)))\n",
            "(let ((x 1)) (progn (a) (b)))\n",
            "(dolist (x xs) (progn (a) (b)))\n",
            "(defun f (x) (progn (a) (b)))\n",
        ] {
            assert_eq!(count(source), 1, "no finding for `{source}`");
        }
    }

    // ----- the corruption this rule shipped with ---------------------------

    /// `` `(progn …) `` is the standard shape of a multi-form macro expansion,
    /// not a body progn. Splicing it evaluated the forms at expansion time and,
    /// because the wrapper carried the backquote away with it, left commas
    /// outside any backquote — source SBCL refuses to read.
    #[test]
    fn a_quasiquoted_progn_is_the_macro_expansion_not_the_body() {
        assert_eq!(count("(defmacro m (x) `(progn (f ,x) (g ,x)))\n"), 0);
    }

    /// A `,(progn …)` sits in the `when`'s body slot but runs at expansion
    /// time and contributes its *value*; splicing it would turn two
    /// expansion-time calls into two emitted forms.
    #[test]
    fn an_unquoted_progn_in_body_position_is_not_a_body_progn() {
        assert_eq!(
            count("(defmacro m (x) `(when c ,(progn (f x) (g x))))\n"),
            0
        );
    }

    // ----- negative controls: quoted data is not rewritten ------------------

    #[test]
    fn a_hard_quoted_progn_in_body_position_is_data() {
        assert_eq!(count("(defparameter *forms* '(progn (a) (b)))\n"), 0);
    }

    /// The case the per-child prefix test cannot see: the quote sits on the
    /// enclosing form, and the progn under it is bare.
    #[test]
    fn a_body_progn_inside_a_hard_quoted_form_is_data() {
        assert_eq!(
            count("(defparameter *forms* '(when c (progn (a) (b))))\n"),
            0
        );
    }

    #[test]
    fn a_body_progn_under_a_long_hand_quote_form_is_data() {
        assert_eq!(
            count("(defparameter *forms* (quote (when c (progn (a) (b)))))\n"),
            0
        );
    }

    /// Symmetric to the one above: an unquote escapes a quasiquote back to
    /// code, and the guard must not treat "has a data ancestor" as data.
    #[test]
    fn an_unquote_inside_a_quasiquote_is_still_code() {
        assert_eq!(count("(defmacro m () `(x ,(when c (progn (a) (b)))))\n"), 1);
    }

    #[test]
    fn a_file_with_no_body_progn_produces_nothing() {
        assert_eq!(count("(when c (a) (b))\n"), 0);
        assert_eq!(count(""), 0);
    }
}
