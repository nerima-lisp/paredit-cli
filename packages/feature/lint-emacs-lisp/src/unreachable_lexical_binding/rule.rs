//! `elisp-unreachable-lexical-binding`: a `lexical-binding` setting Emacs
//! will never read.
//!
//! Every other file variable can be set from a `Local Variables:` block at the
//! end of the file. This one cannot: by the time the reader reaches that
//! block the whole file has already been read, under the default, which is
//! dynamic binding. A `;; lexical-binding: t` down there looks exactly like a
//! setting that works and does nothing at all.
//!
//! The same is true of any `-*- … -*-` header on a line other than the first
//! (or the second, after a `#!` line).

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{Fixability, HeadFilter, RuleCategory, RuleMeta, Severity};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::emacs_lisp::{EmacsLispLexicalBinding, emacs_lisp_file_header};
use paredit_core_syntax::sexpr::{ByteOffset, ByteSpan, ExpressionView};

pub const META: RuleMeta = RuleMeta::new(
    "elisp-unreachable-lexical-binding",
    RuleCategory::Suspicious,
    Severity::Error,
    "a lexical-binding setting outside the first line, which Emacs never reads",
    Fixability::ReportOnly,
);

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
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
        // A setting the file header already found is the one Emacs reads.
        if emacs_lisp_file_header(source).lexical_binding() != EmacsLispLexicalBinding::Absent {
            return Ok(());
        }

        let header_end = header_line_end(source);
        for (offset, line) in line_offsets(source) {
            if offset < header_end || !mentions_lexical_binding(line) {
                continue;
            }

            let span = ByteSpan::new(
                ByteOffset::new(offset),
                ByteOffset::new(offset + line.len()),
            );
            sink.report(
                span,
                "`lexical-binding` is read from the first line only, so this \
                 setting has no effect and the file is evaluated with dynamic \
                 binding",
            );
        }
        Ok(())
    }
}

/// The byte just past the line whose header Emacs would read.
fn header_line_end(source: &str) -> usize {
    let mut end = source.find('\n').map_or(source.len(), |index| index + 1);
    if source.starts_with("#!") {
        end += source
            .get(end..)
            .and_then(|rest| rest.find('\n'))
            .map_or(source.get(end..).map_or(0, str::len), |index| index + 1);
    }
    end
}

fn line_offsets(source: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut offset = 0;
    source.split('\n').map(move |line| {
        let start = offset;
        offset += line.len() + 1;
        (start, line)
    })
}

/// Whether a line sets `lexical-binding`, in either of the two spellings a
/// file variable can take.
fn mentions_lexical_binding(line: &str) -> bool {
    let Some(index) = line.find("lexical-binding") else {
        return false;
    };
    // A prose mention in a docstring or comment is not a setting; a colon
    // after the name is what makes it one.
    line[index + "lexical-binding".len()..]
        .trim_start()
        .starts_with(':')
}
