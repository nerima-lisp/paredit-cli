//! `elisp-missing-lexical-binding`: a file whose first line does not enable
//! lexical binding.
//!
//! Emacs reads this setting from line 1 and nowhere else. Without it the whole
//! file is evaluated under *dynamic* binding, which is not a stylistic
//! difference: every `let` variable becomes readable and writable by every
//! function the body calls, closures stop capturing, and a helper that happens
//! to share a parameter name with a caller silently sees the caller's value.
//!
//! Nothing about the code changes to say so. That is why this is worth a rule
//! rather than a convention — the failure is invisible in the forms and shows
//! up as a bug somewhere else entirely.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{Fixability, HeadFilter, RuleCategory, RuleMeta, Severity};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::emacs_lisp::{EmacsLispLexicalBinding, emacs_lisp_file_header};
use paredit_core_syntax::sexpr::{ByteOffset, ByteSpan, ExpressionView};

pub const META: RuleMeta = RuleMeta::new(
    "elisp-missing-lexical-binding",
    RuleCategory::Suspicious,
    Severity::Warning,
    "an Emacs Lisp file whose first line does not set lexical-binding",
    Fixability::ReportOnly,
);

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        // The subject is a comment on line 1, so there is no node to key on.
        HeadFilter::WholeTree
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::EMACS_LISP_ONLY
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        _view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        let source = context.source();
        if source.trim().is_empty() {
            return Ok(());
        }

        let header = emacs_lisp_file_header(source);
        // `lexical-binding: nil` is reported by `elisp-missing-lexical-binding`
        // only when it is absent: a file that turns the flag off has said what
        // it meant, and some files genuinely need dynamic binding.
        if header.lexical_binding() != EmacsLispLexicalBinding::Absent {
            return Ok(());
        }

        let end = source.find('\n').unwrap_or(source.len()).min(source.len());
        let span = ByteSpan::new(ByteOffset::new(0), ByteOffset::new(end));

        sink.report(
            span,
            "no `lexical-binding` setting on the first line, so this file is \
             evaluated with dynamic binding; add `-*- lexical-binding: t -*-`",
        );
        Ok(())
    }
}
