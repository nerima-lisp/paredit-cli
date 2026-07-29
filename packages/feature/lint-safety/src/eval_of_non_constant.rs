//! `eval-of-non-constant`: evaluating something the source does not show.
//!
//! `(eval form)` runs whatever `form` turns out to be. If any part of `form`
//! came from outside the program — a config file, a request, a filename — the
//! caller has handed control of the process to whoever wrote it.
//!
//! The rule reports the *unknowable* case and only that. `(eval '(setup))` is
//! a constant the reader can see, and is left alone; `(eval x)` is not, and is
//! reported whether or not `x` is in fact trustworthy. That asymmetry is
//! deliberate: a rule about untrusted input cannot tell trusted from untrusted
//! and must not pretend to, so it reports the shape that *could* be untrusted
//! and lets a suppression comment record the judgement.
//!
//! Report-only. There is no mechanical rewrite; the answer is usually a lookup
//! table keyed by a validated symbol.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionView, ReaderPrefix};
use paredit_core_syntax::view_query::{atom_text, list_head, symbol_in};

pub const META: RuleMeta = RuleMeta::new(
    "eval-of-non-constant",
    RuleCategory::Security,
    Severity::Warning,
    "an eval, intern, or read-from-string whose argument is not a literal the source shows",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "These operators turn data into code or into symbols. When the data is computed, nothing \
         in the source says what will run or what will be interned, and any path from outside the \
         program to that argument is a path to arbitrary execution or to unbounded symbol-table \
         growth.",
    )
    .with_example(
        "(eval (read-from-string request))",
        "(funcall (or (gethash request *handlers*) #'reject))",
    )
    .with_caveat(
        "A quoted or self-evaluating argument — `(eval '(setup))`, `(intern \"CONSTANT\")` — is \
         visible in the source and is never reported.",
    ),
);

const HEADS: [NormalizedHead; 4] = [
    NormalizedHead::new("eval"),
    NormalizedHead::new("intern"),
    NormalizedHead::new("read-from-string"),
    NormalizedHead::new("find-symbol"),
];

/// One evaluation of an argument the source does not show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicEvaluation {
    pub span: ByteSpan,
    pub operator: String,
}

/// Whether `view` is something the reader can see the whole value of.
///
/// A quoted form, a string, a number, a keyword, `t`, or `nil`. Anything else —
/// a variable, a call — is a value only the runtime knows.
#[must_use]
pub fn is_visible_constant(view: &ExpressionView) -> bool {
    if view.reader_prefixes.contains(&ReaderPrefix::Quote) {
        return true;
    }
    if view.kind != ExpressionKind::Atom {
        return false;
    }
    let Some(text) = atom_text(view) else {
        return false;
    };
    text.starts_with('"')
        || text.starts_with(':')
        || text.starts_with("#\\")
        || text.eq_ignore_ascii_case("t")
        || text.eq_ignore_ascii_case("nil")
        || text.starts_with(|c: char| c.is_ascii_digit())
        || (text.len() > 1
            && text.starts_with(['+', '-'])
            && text[1..].starts_with(|c: char| c.is_ascii_digit()))
}

/// Reads one call and reports the dynamic evaluation it performs.
#[must_use]
pub fn examine(view: &ExpressionView) -> Option<DynamicEvaluation> {
    let head = list_head(view)?;
    if !symbol_in(head, &["eval", "intern", "read-from-string", "find-symbol"]) {
        return None;
    }
    let argument = view.children.get(1)?;
    (!is_visible_constant(argument)).then(|| DynamicEvaluation {
        span: view.span,
        operator: paredit_core_syntax::view_query::unqualified(head).to_ascii_lowercase(),
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
                    "{} is given a computed argument, so the source does not say what it will \
                     evaluate; any path from outside the program reaches it",
                    found.operator
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

    fn operator(input: &str) -> Option<String> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let view = tree
            .select_path(&Path::root_child(0))
            .expect("root form")
            .view();
        examine(&view).map(|found| found.operator)
    }

    #[test]
    fn flags_an_eval_of_a_computed_form() {
        assert_eq!(operator("(eval form)"), Some("eval".to_owned()));
        assert_eq!(
            operator("(eval (read-from-string input))"),
            Some("eval".to_owned())
        );
    }

    #[test]
    fn flags_every_operator_in_the_family() {
        assert_eq!(operator("(intern name)"), Some("intern".to_owned()));
        assert_eq!(
            operator("(read-from-string payload)"),
            Some("read-from-string".to_owned())
        );
        assert_eq!(
            operator("(find-symbol name)"),
            Some("find-symbol".to_owned())
        );
    }

    #[test]
    fn does_not_flag_a_quoted_argument() {
        assert_eq!(operator("(eval '(setup))"), None);
        assert_eq!(operator("(eval 'my-symbol)"), None);
    }

    #[test]
    fn does_not_flag_a_self_evaluating_argument() {
        assert_eq!(operator(r#"(intern "CONSTANT")"#), None);
        assert_eq!(operator("(eval 42)"), None);
        assert_eq!(operator("(eval -1)"), None);
        assert_eq!(operator("(eval :key)"), None);
        assert_eq!(operator("(eval t)"), None);
        assert_eq!(operator("(eval nil)"), None);
    }

    #[test]
    fn does_not_flag_a_call_with_no_argument() {
        assert_eq!(operator("(eval)"), None);
    }

    #[test]
    fn a_backquoted_template_is_not_a_visible_constant() {
        // A backquote's unquotes are exactly the parts the source does not show.
        assert_eq!(operator("(eval `(run ,command))"), Some("eval".to_owned()));
    }

    #[test]
    fn reads_the_head_case_insensitively_and_through_a_package() {
        assert_eq!(operator("(CL:EVAL form)"), Some("eval".to_owned()));
    }
}
