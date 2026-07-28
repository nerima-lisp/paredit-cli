//! `elisp-autoload-cookie-without-form`: a `;;;###autoload` that autoloads
//! nothing.
//!
//! The cookie is a directive to the `loaddefs` generator, and the generator
//! ignores one it cannot use rather than complaining. Two ways to write one
//! that does nothing:
//!
//! * The cookie is the last thing in the file, or only comments and blank
//!   lines follow it. There is no form to copy.
//! * The cookie sits *inside* a form. `loaddefs` extracts top-level
//!   definitions only, so a cookie in front of a `defun` nested in a
//!   `progn`, an `eval-when-compile`, or a `let` produces no autoload.
//!
//! Either way the function is not autoloaded and the failure surfaces as a
//! `void-function` at some later point in a user's session.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{Fixability, HeadFilter, RuleCategory, RuleMeta, Severity};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::emacs_lisp::{EmacsLispAutoloadPayload, emacs_lisp_autoload_cookies};
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "elisp-autoload-cookie-without-form",
    RuleCategory::Suspicious,
    Severity::Error,
    "a ;;;###autoload cookie with no top-level form after it",
    Fixability::ReportOnly,
);

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        // Correlates comments with top-level forms, so it needs the document.
        HeadFilter::WholeTree
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
        // Only the *top-level* forms, because those are the only ones
        // `loaddefs` copies.
        let top_level: Vec<(usize, usize)> = view
            .children
            .iter()
            .map(|child| (child.span.start().get(), child.span.end().get()))
            .collect();

        for cookie in emacs_lisp_autoload_cookies(context.tree()) {
            // A cookie carrying its own form needs nothing after it: the rest
            // of the line is what gets copied into the generated file.
            if cookie.payload() == EmacsLispAutoloadPayload::InlineForm {
                continue;
            }

            let start = cookie.span().start().get();
            let end = cookie.span().end().get();

            if top_level
                .iter()
                .any(|(form_start, form_end)| *form_start < start && end <= *form_end)
            {
                sink.report(
                    cookie.span(),
                    "this autoload cookie is nested inside a top-level form; \
                     `loaddefs` extracts top-level definitions only, so it \
                     produces no autoload",
                );
                continue;
            }

            if !top_level.iter().any(|(form_start, _)| *form_start >= end) {
                sink.report(
                    cookie.span(),
                    "this autoload cookie is followed by no top-level form, so \
                     `loaddefs` generation silently produces no autoload for it",
                );
            }
        }
        Ok(())
    }
}
