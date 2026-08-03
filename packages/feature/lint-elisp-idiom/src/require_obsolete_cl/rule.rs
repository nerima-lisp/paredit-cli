//! `elisp-require-obsolete-cl`: `(require 'cl)` and `(require 'cl-compat)`.
//!
//! Verified against GNU Emacs 31.0.91: both files live in `lisp/obsolete/`,
//! and loading either prints `Package cl is deprecated` /
//! `Package cl-compat is deprecated` on standard error. `cl.el` exists only to
//! define the unprefixed aliases (`loop`, `flet`, `incf`, `first`, …) that
//! `cl-lib` replaced with `cl-`-prefixed names.
//!
//! This is the `require` half of the same defect `elisp-obsolete-cl-alias`
//! reports at the *use* site. That rule keys on fourteen unprefixed macro
//! heads and cannot see the `require` at all, so a file that requires `cl` and
//! uses only unprefixed *functions* — `remove-if`, `find-if`, `subseq` — goes
//! unreported today.
//!
//! Deliberately **not** fixable. Rewriting the `require` to `cl-lib` breaks
//! the file: every unprefixed name in it stops resolving. The repair is a
//! rename of each use site, which is a different edit from this one, and a
//! `Fixability::Fixable` here would offer to make a working file stop working.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::shared::{designated_symbol, is_unevaluated_at, list_head};

pub const META: RuleMeta = RuleMeta::new(
    "elisp-require-obsolete-cl",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a require of the obsolete cl package, which prints a deprecation notice when loaded",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "`cl.el` and `cl-compat.el` ship from `lisp/obsolete/` and announce themselves as \
         deprecated on load. They exist only to define the unprefixed aliases that `cl-lib` \
         replaced with `cl-`-prefixed names.",
    )
    .with_example(
        "(require 'cl)\n(loop for x in xs collect x)",
        "(require 'cl-lib)\n(cl-loop for x in xs collect x)",
    )
    .with_caveat(
        "No fix is offered. Changing the `require` alone stops every unprefixed name in the file \
         from resolving; the repair is to rename the uses, which is a separate edit.",
    ),
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("require")];

/// The two obsolete features, and what replaces each.
///
/// `cl-lib`, `cl-macs`, `cl-seq` and `cl-extra` are current and are not here.
const OBSOLETE: [(&str, &str); 2] = [("cl", "cl-lib"), ("cl-compat", "cl-lib")];

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
        if list_head(view) != Some("require") {
            return Ok(());
        }
        let Some(feature) = view.children.get(1) else {
            return Ok(());
        };
        // `(require foo)` requires whatever `foo` evaluates to, which this
        // cannot read; only a literal `'cl` names the obsolete package.
        let Some(name) = designated_symbol(feature) else {
            return Ok(());
        };
        let Some(&(_, replacement)) = OBSOLETE.iter().find(|(obsolete, _)| *obsolete == name)
        else {
            return Ok(());
        };
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }

        sink.report(
            view.span,
            format!(
                "{name} is obsolete and prints a deprecation notice when \
                 loaded; {replacement} replaces it, with every name \
                 `cl-`-prefixed"
            ),
        );
        Ok(())
    }
}
