//! `lfe-clause-after-catch-all`: a clause that can never run because an
//! earlier clause in the same form matches everything.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::dead_clause::domain::{self, examine};
use crate::support::node_context;

pub const META: RuleMeta = RuleMeta::new(
    "lfe-clause-after-catch-all",
    RuleCategory::DeadCode,
    Severity::Warning,
    "a clause that can never run because an earlier clause always matches",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "LFE matches clauses in order, so a clause written after one that matches everything can \
         never run. The compiler says so itself: LFE 2.2.0 on OTP 27.3.4.15 reports \"this clause \
         cannot match because a previous clause always matches\" for `case`, `receive` and \
         matching `defun` alike. A catch-all is `_`, or a bare variable — LFE binds a fresh \
         variable in a pattern rather than comparing against an existing binding, so a bare name \
         always matches whatever else is in scope.",
    )
    .with_example(
        "(case x\n  ('one 1)\n  (_ 'fallback)\n  ('two 2))",
        "(case x\n  ('one 1)\n  ('two 2)\n  (_ 'fallback))",
    )
    .with_caveat(
        "A clause carrying a `when` guard is never treated as a catch-all, because the guard can \
         fail. The compiler agrees: a guarded bare-variable clause produces no warning.",
    )
    .with_caveat(
        "A repeated variable in an argument list constrains those arguments to be equal, so \
         `((x x) …)` is not a catch-all and the clause after it is reachable. Measured: the \
         compiler emits no warning for it.",
    )
    .with_caveat(
        "`(defun name (a b) body)` is the traditional single-clause form and has no clause list. \
         This rule distinguishes it from the matching form with LFE's own test, \
         `lfe_lib:is_symb_list` — every element of the argument list being an unquoted atom.",
    ),
);

/// One head per entry of [`domain::CLAUSE_FORMS`], written out because
/// `NormalizedHead::new` is `const` and the table is not a `const` iterator.
///
/// `head_key` is verbatim for LFE — there is no case folding — so these must
/// match the source spelling byte for byte. `match-lambda` in particular is
/// hyphenated, not `matchlambda` or `match_lambda`.
///
/// [`tests::every_clause_form_has_a_head`] pins that this array and
/// `CLAUSE_FORMS` stay the same size, because a form added to the domain
/// without a head here would simply never be dispatched.
const HEADS: [NormalizedHead; 4] = [
    NormalizedHead::new("case"),
    NormalizedHead::new("receive"),
    NormalizedHead::new("match-lambda"),
    NormalizedHead::new("defun"),
];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        // Reads the domain's own table, so the head set and the dialect gate
        // cannot drift apart in a later edit.
        RuleDialectScope::new(&domain::DIALECTS)
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        // Cheap first: `examine` reads only this form's own clause list and
        // answers "no" for every ordinary `defun`, which is the overwhelming
        // majority of what the head index hands over.
        let dead = examine(context.dialect(), view);
        if dead.is_empty() {
            return Ok(());
        }
        // Only now, with findings otherwise ready, the single root-view
        // descent. It materializes the whole document, so asking before
        // `examine` would charge every visited `defun` for a walk that almost
        // always answers "no" — the ordering mistake that measured
        // 450843 ns/call against 28 ns/call in an earlier batch.
        // A `(case …)` inside a quoted list is data, and one inside a
        // `defsyntax` rule is a *template* whose symbols are pattern
        // variables rather than the bare variables they look like. Both were
        // real false positives before this gate existed.
        if node_context(context.tree(), view.span).suppresses_findings() {
            return Ok(());
        }
        for item in dead {
            sink.report(
                item.span,
                format!(
                    "this clause can never run: an earlier clause of this `{}` matches \
                     everything; move the catch-all last",
                    item.form.head()
                ),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A form in `CLAUSE_FORMS` with no matching `NormalizedHead` would never
    /// be dispatched, and every unit test calling `examine` directly would
    /// still pass.
    #[test]
    fn every_clause_form_has_a_head() {
        assert_eq!(HEADS.len(), domain::CLAUSE_FORMS.len());
        for form in domain::CLAUSE_FORMS {
            assert!(
                HEADS.iter().any(|head| head.as_str() == form.head()),
                "no head indexed for {form:?}"
            );
        }
    }

    #[test]
    fn the_rule_is_scoped_to_lfe_only() {
        assert_eq!(domain::DIALECTS.len(), 1);
    }
}
