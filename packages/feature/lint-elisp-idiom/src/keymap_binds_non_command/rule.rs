//! `elisp-keymap-binds-non-command`: a key bound to a function that is not a
//! command.
//!
//! `define-key` and its relatives accept any object at all, so binding a plain
//! `defun` is silent at load time and at byte-compile time. Verified against
//! GNU Emacs 31.0.91: `(commandp 'f)` is `nil` for a `defun` with no
//! `(interactive)`, `define-key` on it is accepted without a word, and
//! byte-compiling a file that does exactly this produces no warning. The
//! failure surfaces only when a user presses the key.
//!
//! Reported only when the bound symbol is defined by a *top-level* `defun` in
//! the same file. A command that lives in another file, or behind an
//! `autoload`, cannot be judged from here, and guessing would report the
//! ordinary case of binding a key to somebody else's command.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::shared::{Definition, designated_symbol, find_definition, is_unevaluated_at, list_head};

pub const META: RuleMeta = RuleMeta::new(
    "elisp-keymap-binds-non-command",
    RuleCategory::Suspicious,
    Severity::Error,
    "a key bound to a same-file defun that has no (interactive), so the binding cannot run",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "A key binding is dispatched through `command-execute`, which requires `commandp`. A \
         `defun` with no `(interactive)` is not `commandp`, and `define-key` accepts it anyway — \
         so the binding loads, byte-compiles and installs cleanly, and fails only when the key is \
         pressed.",
    )
    .with_example(
        "(defun my-go () (message \"hi\"))\n(define-key m (kbd \"C-c a\") #'my-go)",
        "(defun my-go () (interactive) (message \"hi\"))\n(define-key m (kbd \"C-c a\") #'my-go)",
    )
    .with_caveat(
        "Only a symbol this file itself defines at top level is reported. A binding to a command \
         from another file, to a keymap, to a lambda, or to a computed value is left alone.",
    ),
);

/// Every binder head, and where its `DEFINITION` argument sits.
///
/// `(define-key KEYMAP KEY DEF)` and `(global-set-key KEY DEF)` differ by
/// exactly the leading keymap, which is why this is a table rather than a
/// constant index — reading `global-set-key`'s KEY as its DEF would report the
/// `kbd` call and never the function.
const BINDERS: [(&str, usize); 8] = [
    ("define-key", 3),
    ("define-key-after", 3),
    ("keymap-set", 3),
    ("keymap-global-set", 2),
    ("keymap-local-set", 2),
    ("global-set-key", 2),
    ("local-set-key", 2),
    ("keymap-substitute", 3),
];

const HEADS: [NormalizedHead; 8] = [
    NormalizedHead::new("define-key"),
    NormalizedHead::new("define-key-after"),
    NormalizedHead::new("keymap-set"),
    NormalizedHead::new("keymap-global-set"),
    NormalizedHead::new("keymap-local-set"),
    NormalizedHead::new("global-set-key"),
    NormalizedHead::new("local-set-key"),
    NormalizedHead::new("keymap-substitute"),
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
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        // The table lookup is load-bearing twice over: it says *which* binder
        // this is, and so where its DEFINITION argument sits. See
        // `shared::emacs_lisp_operator` for why the comparison is exact rather
        // than case-folded.
        let Some(head) = list_head(view) else {
            return Ok(());
        };
        let Some(&(_, definition_index)) = BINDERS.iter().find(|(binder, _)| *binder == head)
        else {
            return Ok(());
        };
        let Some(bound) = view.children.get(definition_index) else {
            return Ok(());
        };
        // A bare symbol passes a *value*, a lambda is already a function, and
        // a computed expression cannot be read. Only `'f` and `#'f` name a
        // function this rule can go and look up.
        let Some(name) = designated_symbol(bound) else {
            return Ok(());
        };

        let Some((definition_view, shape)) = find_definition(context.tree(), name) else {
            return Ok(());
        };
        let definition = Definition {
            view: &definition_view,
            shape,
        };
        if definition.interactive_header().is_some() {
            return Ok(());
        }
        // Last, because it is the only check that costs more than a field
        // read: a `(define-key …)` inside `'(…)` is a list of symbols.
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }

        sink.report(
            bound.span,
            format!(
                "{name} has no (interactive), so it is not a command and this \
                 binding will fail when the key is pressed"
            ),
        );
        Ok(())
    }
}
