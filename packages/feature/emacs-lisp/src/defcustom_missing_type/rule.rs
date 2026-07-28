//! `elisp-defcustom-missing-type`: a `defcustom` with no `:type`.
//!
//! `:type` is what gives the option a widget. Without one, Customize falls
//! back to editing the raw sexp — the option still works from Lisp, and is
//! effectively unusable through the interface it was declared for. Since the
//! whole reason to write `defcustom` rather than `defvar` is that interface,
//! an option without a type has paid the cost and skipped the benefit.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::emacs_lisp::EmacsLispOperator;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::shared::{atom_text, emacs_lisp_operator, has_keyword_argument};

pub const META: RuleMeta = RuleMeta::new(
    "elisp-defcustom-missing-type",
    RuleCategory::Malformed,
    Severity::Warning,
    "a defcustom with no :type, which leaves Customize editing a raw sexp",
    Fixability::ReportOnly,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("defcustom")];

/// `(defcustom NAME VALUE DOC &rest ARGS)`: the keyword tail starts at 4.
const KEYWORD_START: usize = 4;

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::EMACS_LISP_ONLY
    }

    fn check(
        &self,
        _context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        if emacs_lisp_operator(view) != Some(EmacsLispOperator::Defcustom) {
            return Ok(());
        }
        // A form too short to hold a docstring is malformed in a way this rule
        // has nothing useful to say about.
        if view.children.len() < KEYWORD_START {
            return Ok(());
        }
        if has_keyword_argument(view, KEYWORD_START, ":type") {
            return Ok(());
        }

        let name = view.children.get(1).and_then(atom_text).unwrap_or("?");
        sink.report(
            view.span,
            format!(
                "defcustom {name} has no :type, so Customize cannot offer a \
                 widget for it"
            ),
        );
        Ok(())
    }
}
