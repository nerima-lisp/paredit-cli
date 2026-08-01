//! `commented-repl-transcript`: a comment block shaped like a pasted REPL session.
//!
//! The analysis lives in [`crate::commented_repl_transcript::domain`], which also backs the
//! standalone `inspect commented-repl-transcript` command; this module only registers it with
//! the lint suite and phrases its findings.
//!
//! Deliberately `Fixability::ReportOnly` — see the fuller rationale in
//! [`crate::commented_repl_transcript::domain`]'s module doc: this project's
//! own history includes a write command that silently dropped every comment
//! in a file, because comments live outside the node tree a rewrite walks.

use paredit_core_lint_engine::LintResult;

use crate::commented_repl_transcript::domain::examine;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{Fixability, HeadFilter, RuleCategory, RuleMeta, Severity};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "commented-repl-transcript",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a comment block shaped like a pasted REPL session",
    Fixability::ReportOnly,
);

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        // This rule's subject is comments, not nodes — there is nothing for a
        // per-node filter to narrow. `check` runs once per file and reads
        // `context.tree().comments()` directly.
        HeadFilter::WholeTree
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        _view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut scanned_block_count = 0;
        let mut items = Vec::new();
        examine(
            context.tree(),
            context.source(),
            context.path(),
            &mut scanned_block_count,
            &mut items,
        );
        for item in items {
            sink.report(
                item.span,
                format!(
                    "{} consecutive comment lines look like a pasted REPL transcript",
                    item.line_count
                ),
            );
        }
        Ok(())
    }
}
