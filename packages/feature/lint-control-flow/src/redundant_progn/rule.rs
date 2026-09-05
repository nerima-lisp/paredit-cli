//! `redundant-progn`: a progn that is empty or wraps a single form (progn X is just X).
//!

use paredit_core_lint_engine::LintResult;

use crate::redundant_progn::domain::examine_progn;
use crate::support::is_hard_quoted_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "redundant-progn",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a progn that is empty or wraps a single form (progn X is just X)",
    Fixability::Fixable,
);

/// `examine_progn` only ever matches a `progn` head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("progn")];

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
        let mut progn_form_count = 0;
        let mut items = Vec::new();
        examine_progn(view, &mut progn_form_count, &mut items);
        // `'(progn x)` is a two-element list, not a wrapper: unwrapping it
        // rewrites the program's data. A quasiquote is deliberately *not*
        // suppressed — `` `(progn ,x) `` is a macro template whose progn is
        // emitted as code, and the fix below keeps its backquote attached.
        //
        // Asked only once the walk above already produced a finding, so the
        // ordinary zero-finding file never reaches the enclosing-form descent.
        if !items.is_empty() && is_hard_quoted_at(context.tree(), view.span) {
            return Ok(());
        }
        for item in items {
            let span = item.span;
            let fix = {
                // An empty progn is `nil`; a single-form progn becomes that form,
                // copied verbatim from source to preserve reader prefixes/spacing.
                let replacement = item
                    .inner_span
                    .map_or_else(|| "nil".to_owned(), |span| context.slice(span).to_owned());

                // `content_span`, not `span`: the latter starts at this form's
                // *own* reader prefixes, and replacing it would delete them.
                // `` `(progn ,x) `` has to become `` `,x ``, not `,x`, which is
                // a reader error — the backquote is what makes the comma legal.
                // The two spans coincide on a form with no prefix, which is
                // every progn in ordinary code.
                RuleFix::single(
                    view.content_span,
                    replacement,
                    "Unwrap the redundant progn".to_owned(),
                )
            };
            let detail = if item.body_form_count == 0 {
                "an empty progn is nil".to_owned()
            } else {
                "progn wraps a single form; it is equivalent to that form".to_owned()
            };

            sink.report_fixed(span, format!("redundant progn: {detail}"), fix);
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
    fn still_fires_on_an_ordinary_unquoted_progn() {
        assert_eq!(
            fixed("(defun f () (progn (g)))\n"),
            vec!["(defun f () (g))\n".to_owned()]
        );
    }

    #[test]
    fn still_fires_inside_a_quasiquote_template() {
        // `(progn ,x)` expands to a progn wrapping one form, which is that
        // form. Suppressing the whole quasi half would go quiet here.
        assert_eq!(
            fixed("(defmacro m (x) `(when c (progn ,x)))\n"),
            vec!["(defmacro m (x) `(when c ,x))\n".to_owned()]
        );
    }

    #[test]
    fn still_fires_in_a_defmacro_body() {
        assert_eq!(
            fixed("(defmacro m (x) (progn (build x)))\n"),
            vec!["(defmacro m (x) (build x))\n".to_owned()]
        );
    }

    #[test]
    fn still_fires_on_an_empty_progn() {
        assert_eq!(
            fixed("(defun f () (progn))\n"),
            vec!["(defun f () nil)\n".to_owned()]
        );
    }

    // ----- the corruption this rule shipped with ---------------------------

    /// The measured defect: the fix replaced `view.span`, which begins at the
    /// backquote, so `` `(progn ,form) `` became `,form` — a comma outside any
    /// backquote, which SBCL refuses to read.
    #[test]
    fn a_quasiquoted_progn_keeps_its_backquote() {
        assert_eq!(
            fixed("(defmacro m (form) `(progn ,form))\n"),
            vec!["(defmacro m (form) `,form)\n".to_owned()]
        );
    }

    /// `` `(progn ,@forms) `` is not a one-form progn: `forms` may hold none or
    /// several. The old fix wrote `` `,@forms ``, which SBCL rejects outright
    /// ("not a well-formed backquote expression").
    #[test]
    fn a_spliced_body_form_is_not_one_form() {
        assert_eq!(count("(defmacro m (forms) `(progn ,@forms))\n"), 0);
    }

    #[test]
    fn an_unquoted_progn_keeps_its_comma() {
        assert_eq!(
            fixed("(defmacro m (x) `(list ,(progn (f x))))\n"),
            vec!["(defmacro m (x) `(list ,(f x)))\n".to_owned()]
        );
    }

    // ----- negative controls: quoted data is not rewritten ------------------

    #[test]
    fn a_hard_quoted_progn_is_data_and_is_left_alone() {
        assert_eq!(count("(defparameter *forms* '(progn a))\n"), 0);
    }

    #[test]
    fn a_progn_under_a_long_hand_quote_form_is_left_alone() {
        assert_eq!(count("(defparameter *forms* (quote (progn a)))\n"), 0);
    }

    #[test]
    fn a_progn_nested_deep_inside_quoted_data_is_left_alone() {
        assert_eq!(
            count("(defparameter *table* '((:a (when c (progn x)))))\n"),
            0
        );
    }

    /// A hard quote never clears, so an inner comma does not make this code.
    #[test]
    fn a_comma_inside_a_hard_quote_does_not_re_enter_code() {
        assert_eq!(count("(defparameter *forms* '(a ,(progn b)))\n"), 0);
    }

    /// The symmetric case that proves the guard is on `hard` and not on
    /// "has an ancestor that is data": a quasiquote whose comma escapes back to
    /// code really is code.
    #[test]
    fn an_unquote_inside_a_quasiquote_is_still_code() {
        assert_eq!(count("(defmacro m (x) `(a ,(progn (f x))))\n"), 1);
    }

    #[test]
    fn a_file_with_no_progn_produces_nothing() {
        assert_eq!(count("(defun f (x) (g x))\n"), 0);
        assert_eq!(count(""), 0);
    }
}
