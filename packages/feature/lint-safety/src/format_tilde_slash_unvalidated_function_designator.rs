//! `format-tilde-slash-unvalidated-function-designator`: a format control
//! string the program did not write in full.
//!
//! Common Lisp's `~/package:function/` directive calls the named function to
//! print the argument. The name is read *out of the control string*, so whoever
//! controls the control string chooses which function runs:
//! `(format nil "~/sb-ext:run-program/" x)` is a call, not a print. Every other
//! directive is bounded by what `format` itself does; `~/` is not.
//!
//! A control string spelled out in the source names a function the author
//! chose, so it is never reported. The two shapes that are:
//!
//! - **Assembled.** `(format nil (concatenate 'string prefix "~a") x)` — a value
//!   is spliced into the control string, so a value containing `~/…/` becomes a
//!   directive. This is the same mechanism as `subprocess-string-building`, at
//!   a different sink: there a value escapes into *shell* syntax, here into
//!   *format* syntax.
//! - **Opaque with no arguments.** `(format t message)` names a runtime value as
//!   the control and gives `format` nothing to substitute into it, which is the
//!   shape of "print this string" written with the wrong operator. If the string
//!   came from outside, `~/…/` in it runs. `write-string` is the operator that
//!   does what was meant, and it interprets nothing.
//!
//! Limits, by design — every one of them a false negative taken deliberately,
//! because a security rule that fires on correct code gets switched off:
//!
//! - A control string that is a bare symbol *with* arguments —
//!   `(format stream control a b)` — is not reported. That is the shape of every
//!   logging and reporting wrapper in every codebase, and the rule would be
//!   noise.
//! - The opaque case is only reported for a **bare symbol**. `(format t (if
//!   verbose "~a~%" "~a"))` and `(format s (banner-text))` choose between or
//!   return the program's own control strings far more often than they carry
//!   input, and this rule cannot tell which.
//! - `(format t *usage-banner*)` and `(format t +header+)` are not reported
//!   either: a name the program spells with earmuffs or plus signs is a value
//!   the program itself defined, not input.
//! - A fully literal control string is never read for `~/` at all. Nothing there
//!   is unvalidated, and pattern-matching the directive would only produce
//!   findings on correct pretty-printer code.
//! - `(format "~a~%" x)` — the destination forgotten, so the control string
//!   landed in the destination slot and the *argument* landed in the control
//!   slot — is not reported. That call is malformed, not unsafe, and
//!   `format-missing-destination` already names the typo. A second, security-
//!   flavoured explanation of one missing `t` would be noise.
//!
//! Report-only. Whether the answer is `write-string`, a literal control, or a
//! validated whitelist of printers is a design decision.
//!
//! Scope: Common Lisp only. `~/` is a Common Lisp reader/printer feature.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{atom_text, list_head, symbol_is};

use crate::support::{
    is_literal, is_unevaluated_at, looks_like_program_global, string_build, string_literal,
};

pub const META: RuleMeta = RuleMeta::new(
    "format-tilde-slash-unvalidated-function-designator",
    RuleCategory::Security,
    Severity::Error,
    "a format control string built or supplied at runtime, so a ~/pkg:fn/ directive in it calls an \
     arbitrary function",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "The `~/package:function/` directive names the function `format` calls, and the name is \
         read out of the control string. A control string the program did not write in full \
         therefore chooses which function runs, not just how a value is printed.",
    )
    .with_example("(format t message)", "(write-string message)")
    .with_caveat(
        "A control string passed as a bare symbol *with* format arguments — `(format s control a \
         b)` — is not reported: that is the shape of an ordinary reporting wrapper. Only an \
         assembled control string, or an opaque one with nothing to substitute, is.",
    ),
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("format")];

/// Why one control string was reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlOrigin {
    /// Built at the call site with a value spliced into it.
    Assembled,
    /// A runtime value used as the control with nothing to substitute into it.
    Opaque,
}

impl ControlOrigin {
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::Assembled => {
                "this format control string is assembled at the call site, so a value spliced \
                 into it can introduce directives — including ~/pkg:fn/, which calls the \
                 function it names"
            }
            Self::Opaque => {
                "this format control string is a runtime value and there is nothing to \
                 substitute into it, so format is being used to print a string; a ~/pkg:fn/ in \
                 that string calls the function it names — use write-string"
            }
        }
    }
}

/// One reported control string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnvalidatedControl {
    pub span: ByteSpan,
    pub origin: ControlOrigin,
}

/// Whether `view` is `(formatter "…")`, which is a compiled *literal* control
/// and carries exactly the directives its own string spells.
fn is_formatter_form(view: &ExpressionView) -> bool {
    list_head(view).is_some_and(|head| symbol_is(head, "formatter"))
}

/// Reads one `format` call.
#[must_use]
pub fn examine(view: &ExpressionView, context: &RuleContext<'_>) -> Option<UnvalidatedControl> {
    let head = list_head(view)?;
    if !symbol_is(head, "format") {
        return None;
    }
    // `(format destination control args…)`.
    let destination = view.children.get(1)?;
    let control = view.children.get(2)?;

    // `(format "~a~%" x)` forgot its destination, so what sits in the control
    // slot is the first *argument*. The call is malformed rather than unsafe,
    // and `format-missing-destination` (paredit-feature-lint-string-char) is
    // the rule that says so; adding an injection warning on top of it would be
    // a second, wrong explanation of one typo.
    if string_literal(destination).is_some() {
        return None;
    }

    let origin = classify(control, view.children.len() > 3)?;

    // Asked last, and only once there is something to report: this is the one
    // call in the rule that looks outside the matched node.
    if is_unevaluated_at(context.tree(), view.span) {
        return None;
    }
    Some(UnvalidatedControl {
        span: control.span,
        origin,
    })
}

fn classify(control: &ExpressionView, has_arguments: bool) -> Option<ControlOrigin> {
    if is_literal(control) || is_formatter_form(control) {
        return None;
    }
    if let Some(build) = string_build(control) {
        return (!build.interpolated().is_empty()).then_some(ControlOrigin::Assembled);
    }
    if has_arguments {
        // `(format s control a b)`: an ordinary reporting wrapper.
        return None;
    }
    // Only a bare symbol. A computed control — `(if verbose "~a~%" "~a")`,
    // `(banner-text)` — is far more often the program choosing between its own
    // control strings than it is input, and nothing here can tell them apart.
    let name = atom_text(control)?;
    // A name the program itself defined is not input either.
    if looks_like_program_global(name) {
        return None;
    }
    Some(ControlOrigin::Opaque)
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
        if let Some(found) = examine(view, context) {
            sink.report(found.span, found.origin.describe());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::testing::findings_for_heads;
    use paredit_core_syntax::sexpr::SyntaxTree;

    fn origins(input: &str) -> Vec<ControlOrigin> {
        findings_for_heads(input, &["format"], |view, context| {
            examine(view, context)
                .map(|found| found.origin)
                .into_iter()
                .collect::<Vec<_>>()
        })
    }

    #[test]
    fn flags_an_assembled_control_string() {
        assert_eq!(
            origins(r#"(format nil (concatenate 'string prefix "~a") x)"#),
            vec![ControlOrigin::Assembled]
        );
    }

    #[test]
    fn flags_a_control_string_assembled_by_a_nested_format() {
        assert_eq!(
            origins(r#"(format t (format nil "~a~~a" user-prefix) value)"#),
            vec![ControlOrigin::Assembled]
        );
    }

    #[test]
    fn flags_an_opaque_control_with_nothing_to_substitute() {
        assert_eq!(origins("(format t message)"), vec![ControlOrigin::Opaque]);
        assert_eq!(origins("(format nil reply)"), vec![ControlOrigin::Opaque]);
    }

    // --- near misses ------------------------------------------------------

    #[test]
    fn does_not_flag_a_literal_control_string() {
        assert!(origins(r#"(format t "~a items" n)"#).is_empty());
        assert!(origins(r#"(format nil "~/my-pkg:printer/" x)"#).is_empty());
    }

    #[test]
    fn does_not_flag_a_forwarded_control_with_arguments() {
        // Every logging wrapper in every codebase.
        assert!(origins("(format stream control a b)").is_empty());
    }

    #[test]
    fn does_not_flag_a_program_owned_global_control() {
        assert!(origins("(format t *usage-banner*)").is_empty());
        assert!(origins("(format t +header-format+)").is_empty());
    }

    #[test]
    fn does_not_flag_an_all_literal_assembly() {
        assert!(origins(r#"(format t (concatenate 'string "a" "b"))"#).is_empty());
    }

    #[test]
    fn does_not_flag_a_compiled_formatter() {
        assert!(origins(r#"(format t (formatter "~a") x)"#).is_empty());
    }

    #[test]
    fn does_not_flag_a_computed_control_string() {
        // A deliberate false negative: the program is choosing between, or
        // returning, its own control strings at least as often as it is
        // forwarding input.
        assert!(origins(r#"(format t (if verbose "~a~%" "~a"))"#).is_empty());
        assert!(origins("(format stream (banner-text))").is_empty());
    }

    #[test]
    fn does_not_flag_a_format_that_forgot_its_destination() {
        // `format-missing-destination` owns this one.
        assert!(origins(r#"(format "~a~%" x)"#).is_empty());
    }

    #[test]
    fn does_not_flag_a_call_with_no_control() {
        assert!(origins("(format t)").is_empty());
    }

    // --- quote and string contexts ---------------------------------------

    #[test]
    fn does_not_flag_a_quoted_form() {
        assert!(origins("'(format t message)").is_empty());
        assert!(origins("'(progn (format t message))").is_empty());
        assert!(origins("(quote (format t message))").is_empty());
        assert!(origins("`(format t message)").is_empty());
        assert!(origins("'(a ,(format t message))").is_empty());
    }

    #[test]
    fn flags_an_unquoted_form_inside_a_backquote() {
        assert_eq!(
            origins("`(a ,(format t message))"),
            vec![ControlOrigin::Opaque]
        );
    }

    #[test]
    fn does_not_flag_text_inside_a_string_literal() {
        assert!(origins(r#"(log-it "(format t message)")"#).is_empty());
    }

    #[test]
    fn a_string_that_looks_like_a_call_produces_no_nodes() {
        let input = r#"(log-it "(format t message)")"#;
        let tree = SyntaxTree::parse_with_dialect(
            input,
            paredit_core_syntax::dialect::Dialect::CommonLisp,
        )
        .expect("parse");
        assert_eq!(tree.root_children().len(), 1);
    }
}
