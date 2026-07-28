//! `stream-escapes-with-open-file`: returning the stream the form just closed.
//!
//! `(with-open-file (s path) s)` returns `s` — after `with-open-file` has
//! closed it. Every subsequent operation on the value is on a closed stream.
//!
//! This is the rare resource bug that is unambiguous: there is no reading of
//! `(with-open-file (s p) s)` under which the caller gets something usable, so
//! the rule is an error rather than a warning, and it needs no heuristic.
//!
//! Report-only. What the body *should* return — the parsed contents, a count,
//! the pathname — is the whole question, and the form does not say.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{atom_text, list_head, symbol_in};

pub const META: RuleMeta = RuleMeta::new(
    "stream-escapes-with-open-file",
    RuleCategory::Resource,
    Severity::Error,
    "a with-open-file whose last body form is the stream, which the form has already closed",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "`with-open-file` closes its stream as the body exits, so the value the body returns is a \
         closed stream. Every operation the caller performs on it fails.",
    )
    .with_example(
        "(with-open-file (s path) s)",
        "(with-open-file (s path) (read-lines s))",
    )
    .with_caveat(
        "Only the *returned* value is reported. Passing the stream to a function inside the body \
         is normal and is what the form is for.",
    ),
);

const HEADS: [NormalizedHead; 3] = [
    NormalizedHead::new("with-open-file"),
    NormalizedHead::new("with-open-stream"),
    NormalizedHead::new("with-input-from-string"),
];

/// One escaping stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EscapingStream {
    pub span: ByteSpan,
    pub variable: String,
}

/// Reads one `with-open-*` form and reports the stream its body returns.
#[must_use]
pub fn examine(view: &ExpressionView) -> Option<EscapingStream> {
    let head = list_head(view)?;
    if !symbol_in(
        head,
        &[
            "with-open-file",
            "with-open-stream",
            "with-input-from-string",
        ],
    ) {
        return None;
    }
    let variable = view.children.get(1)?.children.first().and_then(atom_text)?;
    let last = view.children.last()?;
    // The binding form is `children[1]`; a body needs at least one form after
    // it, or there is nothing being returned.
    if view.children.len() < 3 {
        return None;
    }
    let returned = atom_text(last)?;
    returned
        .eq_ignore_ascii_case(variable)
        .then(|| EscapingStream {
            span: view.span,
            variable: variable.to_owned(),
        })
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
        if let Some(found) = examine(view) {
            sink.report(
                found.span,
                format!(
                    "this returns {}, which the form closes on the way out; every operation the \
                     caller performs on it will fail",
                    found.variable
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

    fn variable(input: &str) -> Option<String> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let view = tree
            .select_path(&Path::root_child(0))
            .expect("root form")
            .view();
        examine(&view).map(|found| found.variable)
    }

    #[test]
    fn flags_a_returned_stream() {
        assert_eq!(
            variable("(with-open-file (s path) s)"),
            Some("s".to_owned())
        );
    }

    #[test]
    fn flags_it_after_other_body_forms() {
        assert_eq!(
            variable("(with-open-file (s path) (setup s) s)"),
            Some("s".to_owned())
        );
    }

    #[test]
    fn flags_the_other_with_open_forms() {
        assert_eq!(
            variable("(with-open-stream (s (make-stream)) s)"),
            Some("s".to_owned())
        );
        assert_eq!(
            variable(r#"(with-input-from-string (s "x") s)"#),
            Some("s".to_owned())
        );
    }

    #[test]
    fn does_not_flag_a_body_returning_something_else() {
        assert_eq!(variable("(with-open-file (s path) (read-lines s))"), None);
        assert_eq!(variable("(with-open-file (s path) (use s) t)"), None);
    }

    #[test]
    fn does_not_flag_a_form_with_no_body() {
        assert_eq!(variable("(with-open-file (s path))"), None);
    }

    #[test]
    fn does_not_flag_a_different_variable() {
        assert_eq!(variable("(with-open-file (s path) other)"), None);
    }

    #[test]
    fn reads_the_head_case_insensitively() {
        assert_eq!(
            variable("(WITH-OPEN-FILE (S PATH) S)"),
            Some("S".to_owned())
        );
    }
}
