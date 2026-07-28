//! `unclosed-stream`: an `open` whose stream nothing is guaranteed to close.
//!
//! `(let ((s (open path))) (process s) (close s))` closes the stream on the
//! happy path and leaks it on every other one. If `process` signals, the
//! `close` never runs. `with-open-file` exists precisely to make that
//! impossible, and `unwind-protect` is the manual spelling of the same
//! guarantee.
//!
//! So the rule is not "there is no `close`" — a body with a `close` is the
//! common and still-wrong case — but "there is no *unwinding* close". A body
//! containing an `unwind-protect` is accepted whatever else it does; a body
//! containing only a straight-line `close` is reported, and the message says
//! which of the two situations it found, because they need different fixes.
//!
//! Report-only: converting to `with-open-file` moves the body into a new form
//! and changes what the `let` returns.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{atom_text, calls, for_each_subview, list_head, symbol_in};

pub const META: RuleMeta = RuleMeta::new(
    "unclosed-stream",
    RuleCategory::Resource,
    Severity::Error,
    "a let-bound (open …) stream with no unwind-protect, so a non-local exit leaks the handle",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "A `close` in the straight-line body runs only when the body completes normally. Any \
         condition signalled, `return-from`, or `throw` between the `open` and the `close` skips \
         it and leaks the file handle. `with-open-file` closes on every exit path.",
    )
    .with_example(
        "(let ((s (open path))) (process s) (close s))",
        "(with-open-file (s path) (process s))",
    )
    .with_caveat(
        "A body containing an `unwind-protect` is accepted whatever else it does: that is the \
         manual spelling of the same guarantee.",
    ),
);

const HEADS: [NormalizedHead; 2] = [NormalizedHead::new("let"), NormalizedHead::new("let*")];

/// What the body does about closing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseDiscipline {
    /// No `close` anywhere: the handle leaks on every path.
    Never,
    /// A `close` on the straight-line path only: leaks on a non-local exit.
    OnlyOnSuccess,
}

impl CloseDiscipline {
    #[must_use]
    pub const fn as_message(self) -> &'static str {
        match self {
            Self::Never => "nothing closes it",
            Self::OnlyOnSuccess => {
                "the close runs only when the body completes, so a signalled condition leaks it"
            }
        }
    }
}

/// One leaked stream binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnclosedStream {
    pub span: ByteSpan,
    pub variable: String,
    pub discipline: CloseDiscipline,
}

/// Whether `view` contains an `unwind-protect` anywhere.
fn has_unwind_protect(view: &ExpressionView) -> bool {
    let mut found = false;
    for_each_subview(view, |subview| {
        found |= calls(subview, &["unwind-protect"]);
    });
    found
}

/// Whether `view` contains a `close` naming `variable`.
fn closes(view: &ExpressionView, variable: &str) -> bool {
    let mut found = false;
    for_each_subview(view, |subview| {
        if !calls(subview, &["close"]) {
            return;
        }
        found |= subview
            .children
            .get(1)
            .and_then(atom_text)
            .is_some_and(|name| name.eq_ignore_ascii_case(variable));
    });
    found
}

/// Every leaked stream binding in one `let`/`let*`.
#[must_use]
pub fn examine(view: &ExpressionView) -> Vec<UnclosedStream> {
    let Some(head) = list_head(view) else {
        return Vec::new();
    };
    if !symbol_in(head, &["let", "let*"]) {
        return Vec::new();
    }
    let Some(bindings) = view.children.get(1) else {
        return Vec::new();
    };
    let body = &view.children[2.min(view.children.len())..];

    // One `unwind-protect` covers every stream the `let` binds; there is no
    // reading under which some are protected and others are not.
    if body.iter().any(has_unwind_protect) {
        return Vec::new();
    }

    bindings
        .children
        .iter()
        .filter_map(|binding| {
            let name = binding.children.first().and_then(atom_text)?;
            let init = binding.children.get(1)?;
            if !calls(init, &["open"]) {
                return None;
            }
            let discipline = if body.iter().any(|form| closes(form, name)) {
                CloseDiscipline::OnlyOnSuccess
            } else {
                CloseDiscipline::Never
            };
            Some(UnclosedStream {
                span: binding.span,
                variable: name.to_owned(),
                discipline,
            })
        })
        .collect()
}

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn check(
        &self,
        _context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        for stream in examine(view) {
            sink.report(
                stream.span,
                format!(
                    "{} holds an open stream and {}; use with-open-file, or wrap the body in \
                     unwind-protect",
                    stream.variable,
                    stream.discipline.as_message()
                ),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::{Path, SyntaxTree};

    fn found(input: &str) -> Vec<(String, CloseDiscipline)> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let view = tree
            .select_path(&Path::root_child(0))
            .expect("root form")
            .view();
        examine(&view)
            .into_iter()
            .map(|item| (item.variable, item.discipline))
            .collect()
    }

    #[test]
    fn flags_a_stream_nothing_closes() {
        assert_eq!(
            found("(let ((s (open path))) (process s))"),
            vec![("s".to_owned(), CloseDiscipline::Never)]
        );
    }

    #[test]
    fn flags_a_stream_closed_only_on_success() {
        assert_eq!(
            found("(let ((s (open path))) (process s) (close s))"),
            vec![("s".to_owned(), CloseDiscipline::OnlyOnSuccess)]
        );
    }

    #[test]
    fn accepts_an_unwind_protect() {
        assert!(found("(let ((s (open path))) (unwind-protect (process s) (close s)))").is_empty());
    }

    #[test]
    fn does_not_flag_a_binding_that_is_not_an_open() {
        assert!(found("(let ((s (make-string-output-stream))) (use s))").is_empty());
        assert!(found("(let ((n 0)) (incf n))").is_empty());
    }

    #[test]
    fn reads_let_star_the_same_way() {
        assert_eq!(
            found("(let* ((p path) (s (open p))) (process s))"),
            vec![("s".to_owned(), CloseDiscipline::Never)]
        );
    }

    #[test]
    fn a_close_of_a_different_stream_does_not_count() {
        assert_eq!(
            found("(let ((s (open path))) (close other))"),
            vec![("s".to_owned(), CloseDiscipline::Never)]
        );
    }

    #[test]
    fn every_open_binding_in_one_let_is_reported() {
        let result = found("(let ((a (open p)) (b (open q))) (use a b))");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0, "a");
        assert_eq!(result[1].0, "b");
    }

    #[test]
    fn a_let_with_no_body_is_still_read() {
        assert_eq!(
            found("(let ((s (open path))))"),
            vec![("s".to_owned(), CloseDiscipline::Never)]
        );
    }
}
