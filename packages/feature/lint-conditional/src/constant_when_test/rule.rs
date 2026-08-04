//! `constant-when-test`: a when/unless with a literal t/nil test ((when t b) is (progn b); (when nil b) is nil).
//!
//! The analysis lives in [`crate::constant_when_test::domain`], which also backs the
//! standalone `inspect constant-when-test` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::constant_when_test::domain::examine_when;
use crate::support::is_hard_quoted_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};

pub const META: RuleMeta = RuleMeta::new(
    "constant-when-test",
    RuleCategory::DeadCode,
    Severity::Warning,
    "a when/unless with a literal t/nil test ((when t b) is (progn b); (when nil b) is nil)",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 2] = [NormalizedHead::new("when"), NormalizedHead::new("unless")];

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
        let mut when_form_count = 0;
        let mut items = Vec::new();
        examine_when(view, &mut when_form_count, &mut items);
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
                if item.always_runs {
                    // The body always runs: splice `(when t` / `(unless nil` down to
                    // `(progn`, keeping the body forms verbatim.
                    //
                    // The region starts at `content_span`, not `span`: `span` starts
                    // at this form's *own* reader prefixes, so a region anchored
                    // there swallows them and `` `(when t …) `` loses its
                    // backquote. Only the opening `(when t` is being rewritten, so
                    // the prefix must survive in front of the `(progn`.
                    RuleFix::single(
                        ByteSpan::new(view.content_span.start(), item.splice_span.end()),
                        "(progn".to_owned(),
                        format!(
                            "Rewrite the always-true ({} {} …) as progn",
                            item.head, item.test
                        ),
                    )
                } else {
                    // The body never runs: the whole form is just nil.
                    //
                    // `content_span` for the same reason as above. `` `,(when nil
                    // …) `` collapses to `` `,nil ``, not to a bare `nil` that has
                    // lost the comma binding it to the template.
                    RuleFix::single(
                        view.content_span,
                        "nil".to_owned(),
                        format!("Collapse the dead ({} {} …) to nil", item.head, item.test),
                    )
                }
            };
            let message = if item.always_runs {
                format!(
                    "{} test is the constant {}; the body always runs, so this is a progn",
                    item.head, item.test
                )
            } else {
                format!(
                    "{} test is the constant {}; the body never runs, so this is nil",
                    item.head, item.test
                )
            };

            sink.report_fixed(span, message, fix);
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

    #[test]
    fn still_fires_on_an_ordinary_always_true_when() {
        assert_eq!(
            fixed("(when t (a) (b))\n"),
            vec!["(progn (a) (b))\n".to_owned()]
        );
    }

    /// The always-runs branch rewrites only the `(when t` opening, so the
    /// region it replaces must begin *after* the form's prefix — otherwise the
    /// `(progn` lands on top of the backquote.
    #[test]
    fn a_quasiquoted_always_true_when_keeps_its_backquote() {
        assert_eq!(
            fixed("(defmacro m (x) `(when t ,x))\n"),
            vec!["(defmacro m (x) `(progn ,x))\n".to_owned()]
        );
    }

    #[test]
    fn a_quasiquoted_dead_when_keeps_its_backquote() {
        assert_eq!(
            fixed("(defmacro m (x) `(f (when nil ,x)))\n"),
            vec!["(defmacro m (x) `(f nil))\n".to_owned()]
        );
    }

    /// A hard-quoted `(when t (a))` is a three-element list, not a `when`
    /// form. Rewriting it to `(progn (a))` changes what `*p*` *holds* — the
    /// old behaviour, which this rule shipped with and which the guard now
    /// refuses outright rather than merely spelling differently.
    #[test]
    fn a_hard_quoted_always_true_when_is_not_a_finding_at_all() {
        assert_eq!(count("(defparameter *p* '(when t (a)))\n"), 0);
    }

    /// The same list one level deeper, where the `when` carries no reader
    /// prefix of its own and only an ancestor quotes it.
    #[test]
    fn a_when_inside_a_quoted_ancestor_is_not_a_finding() {
        assert_eq!(count("(defparameter *p* '(f (when t (a))))\n"), 0);
    }

    #[test]
    fn a_long_hand_quoted_when_is_not_a_finding() {
        assert_eq!(count("(defparameter *p* (quote (when t (a))))\n"), 0);
    }

    #[test]
    fn a_variable_test_is_left_alone() {
        assert_eq!(count("(when c (a))\n"), 0);
    }
}
