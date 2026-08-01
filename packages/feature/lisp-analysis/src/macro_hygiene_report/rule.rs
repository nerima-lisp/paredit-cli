//! `macro-variable-capture`: the lint-rule face of `inspect macro-hygiene`.
//!
//! [`crate::macro_hygiene_report::domain`] backs both this rule and the
//! standalone `inspect macro-hygiene` report, and reports five risks; this
//! rule surfaces only [`HygieneRisk::VariableCapture`] through `inspect lint`.
//!
//! Report-only. An automatic gensym rewrite was implemented and then reverted:
//! review found it could corrupt a working macro through unquoting inside a
//! nested quasiquote, shadowing the macro's own lambda-list parameter,
//! rewriting quoted literal data, and emitting Common-Lisp-shaped `let`/`,`
//! syntax into the seven other dialects this rule covers.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::definition::is_macro_expander_definition;
use paredit_core_syntax::sexpr::ExpressionView;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::view_query::list_head;

use crate::macro_hygiene_report::domain::{
    HYGIENE_MODELLED_DIALECTS, HygieneRisk, hygiene_findings_in,
};

pub const META: RuleMeta = RuleMeta::new(
    "macro-variable-capture",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a defmacro template binds a literal name that is not obviously a gensym",
    Fixability::ReportOnly,
);

/// Every macro-definition head [`is_macro_expander_definition`] recognises
/// across [`HYGIENE_MODELLED_DIALECTS`]. A pre-filter only: `check` still
/// confirms the exact dialect/head pairing before doing anything, since e.g.
/// `macro` means something in Fennel and nothing in the other seven.
const HEADS: [NormalizedHead; 4] = [
    NormalizedHead::new("defmacro"),
    NormalizedHead::new("cl-defmacro"),
    NormalizedHead::new("macro"),
    NormalizedHead::new("defsyntax"),
];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::new(&HYGIENE_MODELLED_DIALECTS)
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        let dialect = context.dialect();
        let Some(head) = list_head(view) else {
            return Ok(());
        };
        if !is_macro_expander_definition(dialect, head) {
            return Ok(());
        }
        let Some(name) = view.children.get(1).and_then(atom_symbol_text) else {
            return Ok(());
        };

        for finding in hygiene_findings_in(dialect, view, name) {
            if finding.risk != HygieneRisk::VariableCapture {
                continue;
            }
            let message = format!(
                "variable capture: template binds `{}` to a literal name that is not \
                 obviously a gensym",
                finding.subject
            );
            sink.report(finding.span, message);
        }
        Ok(())
    }
}
