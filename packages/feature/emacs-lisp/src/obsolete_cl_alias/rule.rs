//! `elisp-obsolete-cl-alias`: a `cl.el` name that Emacs 27 removed.
//!
//! `cl.el` provided unprefixed aliases — `loop`, `case`, `flet`, `incf` — and
//! was deleted in Emacs 27.1. A file still using them does not merely warn; it
//! fails to load, because the symbols no longer exist.
//!
//! Most of the renames are mechanical: `loop` became `cl-loop`, `case` became
//! `cl-case`, with identical semantics. Two are not, and the message says so:
//! `flet` and `labels` rebound the *function cell* for a dynamic extent, while
//! `cl-flet` and `cl-labels` create lexical bindings. Code that relied on the
//! old behaviour to stub out a function seen by a callee needs `cl-letf`
//! instead, which is why this rule reports rather than fixes.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::shared::list_head;

pub const META: RuleMeta = RuleMeta::new(
    "elisp-obsolete-cl-alias",
    RuleCategory::Suspicious,
    Severity::Error,
    "an unprefixed cl.el name, removed in Emacs 27.1",
    Fixability::ReportOnly,
);

const HEADS: [NormalizedHead; 14] = [
    NormalizedHead::new("block"),
    NormalizedHead::new("case"),
    NormalizedHead::new("destructuring-bind"),
    NormalizedHead::new("do"),
    NormalizedHead::new("do*"),
    NormalizedHead::new("ecase"),
    NormalizedHead::new("flet"),
    NormalizedHead::new("labels"),
    NormalizedHead::new("letf"),
    NormalizedHead::new("letf*"),
    NormalizedHead::new("loop"),
    NormalizedHead::new("macrolet"),
    NormalizedHead::new("multiple-value-bind"),
    NormalizedHead::new("symbol-macrolet"),
];

/// Whether the rename also changes what the form means.
fn rebinds_the_function_cell(head: &str) -> bool {
    matches!(head, "flet" | "labels")
}

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
        let Some(head) = list_head(view) else {
            return Ok(());
        };
        // The engine matched a *normalized* head, which lowercases; Emacs Lisp
        // does not, so `LOOP` is a name a package may define.
        if !HEADS.iter().any(|known| known.as_str() == head) {
            return Ok(());
        }

        let message = if rebinds_the_function_cell(head) {
            format!(
                "`{head}` was removed in Emacs 27.1; `cl-{head}` replaces it \
                 but binds lexically rather than rebinding the function cell — \
                 use `cl-letf` if a callee must see the replacement"
            )
        } else {
            format!("`{head}` was removed in Emacs 27.1; use `cl-{head}`")
        };

        let span = view.children.first().map_or(view.span, |child| child.span);
        sink.report(span, message);
        Ok(())
    }
}
