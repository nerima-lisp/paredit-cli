//! `elisp-defcustom-missing-group`: a `defcustom` with no `:group`.
//!
//! An option with no group does not appear anywhere in the Customize group
//! tree, so a user who does not already know its name cannot find it. Emacs
//! attaches it to whatever group the last `defgroup` in the file happened to
//! set as current, which makes the placement depend on file order rather than
//! on anything the option says about itself.

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
    "elisp-defcustom-missing-group",
    RuleCategory::Malformed,
    Severity::Warning,
    "a defcustom with no :group, which leaves the option out of the group tree",
    Fixability::ReportOnly,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("defcustom")];

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
        if view.children.len() < KEYWORD_START {
            return Ok(());
        }
        if has_keyword_argument(view, KEYWORD_START, ":group") {
            return Ok(());
        }

        let name = view.children.get(1).and_then(atom_text).unwrap_or("?");
        sink.report(
            view.span,
            format!(
                "defcustom {name} has no :group, so its placement in the \
                 Customize tree depends on file order"
            ),
        );
        Ok(())
    }
}
