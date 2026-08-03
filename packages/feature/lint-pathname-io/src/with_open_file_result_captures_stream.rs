//! `with-open-file-result-captures-stream`: handing the stream back inside
//! something.
//!
//! `(with-open-file (s path) (lambda () (read-line s)))` returns a closure over
//! a stream the form closes on its way out. So does
//! `(with-open-file (s path) (list s (file-length s)))`, and
//! `(with-open-file (s path) (make-instance 'reader :stream s))`. Every one of
//! them defers the failure to whoever calls the closure or reads the slot,
//! which is where it will be hardest to explain.
//!
//! ## What this rule is not
//!
//! `stream-escapes-with-open-file` in `paredit-feature-lint-safety` already
//! reports the *bare* stream: its `examine` requires `atom_text(last)` to equal
//! the bound variable, so it fires on `(with-open-file (s p) s)` and on nothing
//! else. A last body form that is a `lambda` or a constructor call is not an
//! atom, so that rule returns `None` for every shape reported here. The two are
//! the same defect at two levels of indirection, and neither covers the other.
//!
//! ## What is deliberately not reported
//!
//! Passing the stream to a function *inside* the body is what the form is for,
//! so only the value the body *returns* is examined. A closure whose own lambda
//! list rebinds the name shadows the stream and is not capturing it. And a
//! constructor is only reported when the stream is a *direct* argument:
//! `(list (read-line s))` returns a string, not a stream.
//!
//! Report-only. What should have been returned — the parsed contents, a count,
//! a fully-realized list — is the whole question, and the form does not say.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{for_each_subview, list_head, symbol_is};

use crate::support::{head_among, is_unevaluated_at};

pub const META: RuleMeta = RuleMeta::new(
    "with-open-file-result-captures-stream",
    RuleCategory::Resource,
    Severity::Error,
    "a with-open-file whose result is a closure over, or an aggregate holding, the stream it closed",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "`with-open-file` closes its stream as the body exits. A closure that reads the stream, or \
         a list, vector, or instance holding it, survives that exit — so the failure surfaces \
         wherever the value is finally used rather than here, on a stream whose provenance is by \
         then invisible.",
    )
    .with_example(
        "(with-open-file (s path) (lambda () (read-line s)))",
        "(with-open-file (s path) (let ((lines (read-lines s))) (lambda () (pop lines))))",
    )
    .with_caveat(
        "Only the value the body returns is examined; passing the stream to a function inside the \
         body is what the form is for. A closure whose lambda list rebinds the name is shadowing, \
         not capturing, and is not reported.",
    ),
);

const HEADS: [NormalizedHead; 3] = [
    NormalizedHead::new("with-open-file"),
    NormalizedHead::new("with-open-stream"),
    NormalizedHead::new("with-input-from-string"),
];

/// Operators that build a value out of their arguments, so an argument reaches
/// the caller.
///
/// Deliberately short and literal. `append` and `mapcar` also propagate their
/// arguments, but a stream is not a list and putting them here would trade a
/// real finding for a class of false ones.
const CONSTRUCTORS: [&str; 6] = ["list", "list*", "cons", "vector", "values", "make-instance"];

/// The binding forms this rule reads, matching [`HEADS`].
const WITH_OPEN_FORMS: [&str; 3] = [
    "with-open-file",
    "with-open-stream",
    "with-input-from-string",
];

/// How the stream reaches the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capture {
    /// A `lambda` in return position closes over it.
    Closure,
    /// A constructor in return position holds it.
    Aggregate,
}

impl Capture {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Closure => "a closure over",
            Self::Aggregate => "an aggregate holding",
        }
    }
}

/// One captured stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedStream {
    pub span: ByteSpan,
    pub variable: String,
    pub capture: Capture,
}

/// Whether an atom names `variable`, past any reader prefix.
fn names(view: &ExpressionView, variable: &str) -> bool {
    atom_symbol_text(view).is_some_and(|text| text.eq_ignore_ascii_case(variable))
}

/// Whether a lambda's own lambda list rebinds `variable`.
///
/// `(lambda (s) …)` inside `(with-open-file (s …) …)` refers to its parameter,
/// not to the stream, and reporting it would be a false positive on a form that
/// is merely confusingly named.
fn lambda_list_shadows(lambda: &ExpressionView, variable: &str) -> bool {
    let Some(lambda_list) = lambda.children.get(1) else {
        return false;
    };
    lambda_list.children.iter().any(|parameter| {
        // `&optional (s default)` and `&key (s default)` bind `s` too.
        names(parameter, variable)
            || parameter
                .children
                .first()
                .is_some_and(|name| names(name, variable))
    })
}

/// Whether `variable` is referenced anywhere in `view`.
fn references(view: &ExpressionView, variable: &str) -> bool {
    let mut found = false;
    for_each_subview(view, |subview| {
        found |= names(subview, variable);
    });
    found
}

/// Reads one `with-open-*` form and reports the stream its result carries out.
#[must_use]
pub fn examine(view: &ExpressionView) -> Option<CapturedStream> {
    let head = list_head(view)?;
    head_among(head, &WITH_OPEN_FORMS)?;
    // `children[1]` is the binding form; a body needs at least one form after
    // it, or there is nothing being returned.
    if view.children.len() < 3 {
        return None;
    }
    let variable = atom_symbol_text(view.children.get(1)?.children.first()?)?;
    let returned = view.children.last()?;
    let returned_head = list_head(returned)?;

    let capture = if symbol_is(returned_head, "lambda") {
        if lambda_list_shadows(returned, variable) || !references(returned, variable) {
            return None;
        }
        Capture::Closure
    } else if head_among(returned_head, &CONSTRUCTORS).is_some() {
        // A *direct* argument only: `(list (read-line s))` returns a string.
        if !returned.children.iter().skip(1).any(|a| names(a, variable)) {
            return None;
        }
        Capture::Aggregate
    } else {
        return None;
    };

    Some(CapturedStream {
        span: view.span,
        variable: variable.to_owned(),
        capture,
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
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        let Some(found) = examine(view) else {
            return Ok(());
        };
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        sink.report(
            found.span,
            format!(
                "this returns {} {}, which the form closes on the way out; the failure will \
                 surface wherever the value is finally used",
                found.capture.as_str(),
                found.variable
            ),
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::{Path, SyntaxTree};

    fn found(input: &str) -> Option<(String, Capture)> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let view = tree
            .select_path(&Path::root_child(0))
            .expect("root form")
            .view();
        examine(&view).map(|item| (item.variable, item.capture))
    }

    #[test]
    fn flags_a_closure_over_the_stream() {
        assert_eq!(
            found("(with-open-file (s path) (lambda () (read-line s)))"),
            Some(("s".to_owned(), Capture::Closure))
        );
    }

    #[test]
    fn flags_a_sharp_quoted_closure() {
        assert_eq!(
            found("(with-open-file (s path) #'(lambda () (read-line s)))"),
            Some(("s".to_owned(), Capture::Closure))
        );
    }

    #[test]
    fn flags_an_aggregate_holding_the_stream() {
        assert_eq!(
            found("(with-open-file (s path) (list s (file-length s)))"),
            Some(("s".to_owned(), Capture::Aggregate))
        );
        assert_eq!(
            found("(with-open-file (s path) (make-instance 'reader :stream s))"),
            Some(("s".to_owned(), Capture::Aggregate))
        );
        assert_eq!(
            found("(with-open-file (s path) (values s t))"),
            Some(("s".to_owned(), Capture::Aggregate))
        );
    }

    #[test]
    fn flags_the_other_with_open_forms() {
        assert_eq!(
            found("(with-open-stream (s (make-stream)) (lambda () (read s)))"),
            Some(("s".to_owned(), Capture::Closure))
        );
        assert_eq!(
            found(r#"(with-input-from-string (s "x") (cons s nil))"#),
            Some(("s".to_owned(), Capture::Aggregate))
        );
    }

    #[test]
    fn flags_it_after_other_body_forms() {
        assert_eq!(
            found("(with-open-file (s path) (setup s) (lambda () (read-line s)))"),
            Some(("s".to_owned(), Capture::Closure))
        );
    }

    /// The shadowing guard: this lambda's `s` is its own parameter.
    #[test]
    fn does_not_flag_a_lambda_that_rebinds_the_name() {
        assert_eq!(
            found("(with-open-file (s path) (lambda (s) (read-line s)))"),
            None
        );
        assert_eq!(
            found("(with-open-file (s path) (lambda (a &optional (s nil)) (read-line s)))"),
            None
        );
    }

    #[test]
    fn does_not_flag_a_closure_that_never_mentions_the_stream() {
        assert_eq!(found("(with-open-file (s path) (lambda () 42))"), None);
        assert_eq!(
            found("(with-open-file (s path) (lambda (x) (+ x 1)))"),
            None
        );
    }

    #[test]
    fn does_not_flag_an_aggregate_of_values_read_from_the_stream() {
        assert_eq!(
            found("(with-open-file (s path) (list (read-line s) (file-length s)))"),
            None
        );
    }

    #[test]
    fn does_not_flag_a_body_returning_something_else() {
        assert_eq!(found("(with-open-file (s path) (read-lines s))"), None);
        assert_eq!(found("(with-open-file (s path) (use s) t)"), None);
    }

    /// The bare returned stream is `stream-escapes-with-open-file`'s finding,
    /// not this rule's; reporting it here would double-report one defect.
    #[test]
    fn does_not_flag_the_bare_returned_stream() {
        assert_eq!(found("(with-open-file (s path) s)"), None);
    }

    #[test]
    fn does_not_flag_a_form_with_no_body() {
        assert_eq!(found("(with-open-file (s path))"), None);
        assert_eq!(found("(with-open-file)"), None);
    }

    /// The binding form is never read as the returned form.
    ///
    /// With no body, `children.last()` *is* the binding form, and a binding
    /// form whose variable and operator collide looks exactly like a
    /// constructor holding its own stream. Contrived, but it is the only input
    /// that distinguishes the arity guard from the cases above — mutation
    /// testing is how that was found, and the invariant it pins is real: a
    /// binding form is not a body form.
    #[test]
    fn does_not_read_the_binding_form_as_the_result() {
        assert_eq!(found("(with-open-file (cons cons))"), None);
        assert_eq!(found("(with-open-stream (list list))"), None);
    }

    #[test]
    fn reads_the_head_case_insensitively() {
        assert_eq!(
            found("(WITH-OPEN-FILE (S PATH) (LAMBDA () (READ-LINE S)))"),
            Some(("S".to_owned(), Capture::Closure))
        );
    }
}
