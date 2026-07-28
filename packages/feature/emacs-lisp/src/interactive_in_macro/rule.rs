//! `elisp-interactive-in-macro`: an `(interactive …)` at the head of a macro
//! definition.
//!
//! A macro is expanded, never called, so it cannot be a command. `M-x` will
//! not find it and a key binding to it will not work. The form is not an
//! error — it is read as an ordinary call in the macro's body that happens to
//! be first, so it runs at *expansion* time and its return value is discarded.
//!
//! Almost always this means `defmacro` was written where `defun` was meant.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ExpressionKind, ExpressionView};

use crate::shared::{atom_text, emacs_lisp_operator, list_head};

pub const META: RuleMeta = RuleMeta::new(
    "elisp-interactive-in-macro",
    RuleCategory::Suspicious,
    Severity::Error,
    "an (interactive ...) inside a macro definition, which cannot be a command",
    Fixability::ReportOnly,
);

const HEADS: [NormalizedHead; 2] = [
    NormalizedHead::new("defmacro"),
    NormalizedHead::new("cl-defmacro"),
];

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
        let Some(operator) = emacs_lisp_operator(view) else {
            return Ok(());
        };
        let Some(shape) = operator.callable_shape() else {
            return Ok(());
        };
        // Only the macro definitions reach here, and by construction none of
        // them accepts an `(interactive …)` header.
        if shape.accepts_interactive() {
            return Ok(());
        }

        // The header runs from the body start past the docstring and any
        // `(declare …)`; an `(interactive …)` anywhere in that run was meant
        // as a command specification.
        let mut index = shape.body_start_child_index();
        while let Some(child) = view.children.get(index) {
            if child.kind == ExpressionKind::Atom {
                // A docstring, which may precede the header.
                index += 1;
                continue;
            }
            match list_head(child) {
                Some("declare") | Some("cl-declare") => index += 1,
                Some("interactive") => {
                    let name = view.children.get(1).and_then(atom_text).unwrap_or("?");
                    sink.report(
                        child.span,
                        format!(
                            "macro {name} declares itself interactive, but a \
                             macro is expanded rather than called and cannot \
                             be a command; `defun` was probably meant"
                        ),
                    );
                    return Ok(());
                }
                _ => return Ok(()),
            }
        }
        Ok(())
    }
}
