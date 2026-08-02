//! `read-eval-star-rebound-to-t`: `#.` deliberately switched back on.
//!
//! `*read-eval*` is already true when a Lisp starts. Setting it to true
//! therefore never *enables* anything that was not already enabled by default —
//! the only thing it can do is undo a `nil` binding somebody put there on
//! purpose. `(let ((*read-eval* nil)) … (setf *read-eval* t) …)` and
//! `(let ((*read-eval* t)) (load-user-data))` both hand `#.(delete-file "…")` in
//! the input back its ability to run.
//!
//! That is what makes this rule cheap to be sure about: there is no benign
//! reason to write `t` here. Restoring a saved value is spelled by *leaving* the
//! `let`, not by assigning; a program that wants the default writes nothing at
//! all.
//!
//! # Its relationship to `read-without-read-eval-guard`
//!
//! The two are inverses and are kept strictly disjoint, by construction rather
//! than by hope.
//!
//! `read-without-read-eval-guard` anchors on `defun`/`defmethod`/`lambda`, walks
//! down for a `read`-family call, and reports *at the reader* when no enclosing
//! `(let ((*read-eval* nil)) …)` covers it. A `t` binding is not a guard to it,
//! so `(defun f (s) (let ((*read-eval* t)) (read s)))` is already reported there
//! — at the `read`, which is the more actionable place.
//!
//! So this rule stays off exactly that form: a `let` binding `*read-eval*` to
//! `t` is *not* reported when both
//!
//! - its body contains a `read`-family call not already covered by a nested
//!   `nil` binding, and
//! - some enclosing form is a `defun`/`defmethod`/`lambda`, which is what makes
//!   the other rule fire at all.
//!
//! Drop either half and this rule is the only coverage: a `t` binding with no
//! reader under it says nothing to the other rule, and a `t` binding at top
//! level is outside every head it anchors on. Both are reported here.
//!
//! `setf`/`setq` is a different story: no `let` binding is involved, so
//! `read-without-read-eval-guard` cannot see it at any position and there is
//! nothing to defer to.
//!
//! # What else fires on the same line
//!
//! `global-mutation-in-function` (this package, `RuleCategory::Concurrency`)
//! reports `(setf *read-eval* t)` *inside a function body* as an assignment to a
//! special-looking global. That is a different complaint in a different
//! category with a different remedy — it is about shared mutable state and would
//! say the same thing about `(setf *print-base* 16)`. Neither rule subsumes the
//! other, and a `--category security` run sees only this one.
//!
//! Report-only. Removing the rebinding may change what the surrounding code can
//! read, which is a decision about the input format.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{atom_text, list_head, symbol_in, symbol_is};

use crate::support::context_at;

pub const META: RuleMeta = RuleMeta::new(
    "read-eval-star-rebound-to-t",
    RuleCategory::Security,
    Severity::Error,
    "*read-eval* explicitly set or bound to t, re-arming #. evaluation inside read",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "`*read-eval*` is true by default, so writing `t` to it cannot enable anything that was \
         not already on — it can only undo a `nil` binding placed there deliberately. After it, \
         `#.(…)` in whatever is read next executes as part of reading.",
    )
    .with_example(
        "(let ((*read-eval* t)) (read stream))",
        "(let ((*read-eval* nil)) (read stream))",
    )
    .with_caveat(
        "A `let` that binds `*read-eval*` to `t` around a `read` inside a function is left to \
         `read-without-read-eval-guard`, which reports the same defect at the reader itself. The \
         two rules never both fire on one form.",
    ),
);

const HEADS: [NormalizedHead; 4] = [
    NormalizedHead::new("setf"),
    NormalizedHead::new("setq"),
    NormalizedHead::new("let"),
    NormalizedHead::new("let*"),
];

/// The readers that honour `*read-eval*` — the same list
/// `read_without_read_eval_guard` walks for, because agreeing with it is the
/// whole point of consulting them here.
const READERS: [&str; 4] = [
    "read",
    "read-from-string",
    "read-preserving-whitespace",
    "read-delimited-list",
];

/// How `*read-eval*` was set back to true.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rebinding {
    /// `(setf *read-eval* t)` / `(setq *read-eval* t)`.
    Assignment,
    /// `(let ((*read-eval* t)) …)`.
    Binding,
}

impl Rebinding {
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::Assignment => {
                "*read-eval* is already t by default, so assigning t to it can only undo a nil \
                 binding put there on purpose; from here a #.(…) in anything read is executed \
                 while reading"
            }
            Self::Binding => {
                "binding *read-eval* to t re-arms #. evaluation for everything read in this body; \
                 *read-eval* is already t by default, so this can only be undoing a nil binding"
            }
        }
    }
}

/// One rebinding of `*read-eval*` to true.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArmedReadEval {
    pub span: ByteSpan,
    pub kind: Rebinding,
}

fn is_read_eval(name: &str) -> bool {
    symbol_is(name, "*read-eval*")
}

fn is_true(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case("t"))
}

/// Whether one `let` binding sets `*read-eval*` to `value`.
fn binds_read_eval(binding: &ExpressionView, value: fn(&ExpressionView) -> bool) -> bool {
    let Some(name) = binding.children.first().and_then(atom_text) else {
        return false;
    };
    is_read_eval(name) && binding.children.get(1).is_some_and(value)
}

fn is_nil(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case("nil"))
}

/// Whether `view` is a `let`/`let*` that binds `*read-eval*` to nil — the guard
/// that stops the reader search, matching `read_without_read_eval_guard`'s own
/// pruning exactly.
fn establishes_guard(view: &ExpressionView) -> bool {
    let Some(head) = list_head(view) else {
        return false;
    };
    if !symbol_in(head, &["let", "let*"]) {
        return false;
    }
    view.children
        .get(1)
        .is_some_and(|bindings| bindings.children.iter().any(|b| binds_read_eval(b, is_nil)))
}

/// Whether any `read`-family call in `body` is one `read-without-read-eval-guard`
/// would report — i.e. one not already covered by a nested `nil` binding.
///
/// Iterative and bounded to the matched `let`'s own subtree: this is asked once,
/// after a `t` binding has already been found, so a file with no `*read-eval*`
/// in it never reaches this function at all.
fn contains_reportable_reader(body: &[ExpressionView]) -> bool {
    let mut stack: Vec<&ExpressionView> = body.iter().collect();
    while let Some(view) = stack.pop() {
        if establishes_guard(view) {
            continue;
        }
        if list_head(view).is_some_and(|head| symbol_in(head, &READERS)) {
            return true;
        }
        stack.extend(view.children.iter());
    }
    false
}

/// Reads one `setf`/`setq`/`let`/`let*`.
#[must_use]
pub fn examine(view: &ExpressionView, context: &RuleContext<'_>) -> Vec<ArmedReadEval> {
    let Some(head) = list_head(view) else {
        return Vec::new();
    };

    let found = if symbol_in(head, &["setf", "setq"]) {
        assignment(view)
    } else if symbol_in(head, &["let", "let*"]) {
        binding(view, context)
    } else {
        None
    };

    let Some(found) = found else {
        return Vec::new();
    };
    // Asked last, and only once there is something to report.
    if context_at(context.tree(), found.span).unevaluated {
        return Vec::new();
    }
    vec![found]
}

/// `(setf place value place value …)`; every place sits at an odd index.
fn assignment(view: &ExpressionView) -> Option<ArmedReadEval> {
    let mut index = 1;
    while index + 1 < view.children.len() {
        if atom_text(&view.children[index]).is_some_and(is_read_eval)
            && is_true(&view.children[index + 1])
        {
            return Some(ArmedReadEval {
                span: view.span,
                kind: Rebinding::Assignment,
            });
        }
        index += 2;
    }
    None
}

fn binding(view: &ExpressionView, context: &RuleContext<'_>) -> Option<ArmedReadEval> {
    let bindings = view.children.get(1)?;
    let armed = bindings
        .children
        .iter()
        .find(|binding| binds_read_eval(binding, is_true))?;

    // Defer to `read-without-read-eval-guard` on exactly the forms it reports:
    // a reader in this body, inside a definition it anchors on.
    let body = view.children.get(2..).unwrap_or(&[]);
    if contains_reportable_reader(body) && context_at(context.tree(), view.span).inside_definition {
        return None;
    }

    Some(ArmedReadEval {
        span: armed.span,
        kind: Rebinding::Binding,
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
        for found in examine(view, context) {
            sink.report(found.span, found.kind.describe());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::testing::findings_for_heads;

    fn kinds(input: &str) -> Vec<Rebinding> {
        findings_for_heads(input, &["setf", "setq", "let", "let*"], |view, context| {
            examine(view, context)
                .into_iter()
                .map(|found| found.kind)
                .collect::<Vec<_>>()
        })
    }

    #[test]
    fn flags_an_explicit_assignment() {
        assert_eq!(kinds("(setf *read-eval* t)"), vec![Rebinding::Assignment]);
        assert_eq!(kinds("(setq *read-eval* T)"), vec![Rebinding::Assignment]);
    }

    #[test]
    fn flags_an_assignment_in_a_later_setf_pair() {
        assert_eq!(
            kinds("(setf *print-base* 16 *read-eval* t)"),
            vec![Rebinding::Assignment]
        );
    }

    #[test]
    fn flags_an_assignment_inside_a_function() {
        // `global-mutation-in-function` also reports this, for an unrelated
        // reason and in a different category; see this module's header.
        assert_eq!(
            kinds("(defun load-config (s) (setf *read-eval* t) (parse s))"),
            vec![Rebinding::Assignment]
        );
    }

    #[test]
    fn flags_a_binding_with_no_reader_under_it() {
        assert_eq!(
            kinds("(let ((*read-eval* t)) (compute))"),
            vec![Rebinding::Binding]
        );
    }

    #[test]
    fn flags_a_top_level_binding_even_with_a_reader_under_it() {
        // Outside every head `read-without-read-eval-guard` anchors on, so
        // this rule is the only coverage.
        assert_eq!(
            kinds("(let ((*read-eval* t)) (read s))"),
            vec![Rebinding::Binding]
        );
    }

    // --- disjointness with read-without-read-eval-guard -------------------

    #[test]
    fn defers_to_the_guard_rule_inside_a_definition() {
        assert!(kinds("(defun f (s) (let ((*read-eval* t)) (read s)))").is_empty());
        assert!(
            kinds("(defmethod f ((s stream)) (let* ((*read-eval* t)) (read-from-string s)))")
                .is_empty()
        );
        assert!(
            kinds("(lambda (s) (let ((*read-eval* t)) (loop repeat 3 collect (read s))))")
                .is_empty()
        );
    }

    #[test]
    fn does_not_defer_when_a_nested_guard_already_covers_every_reader() {
        // `read-without-read-eval-guard` reports nothing here, so deferring
        // would lose the finding entirely.
        assert_eq!(
            kinds("(defun f (s) (let ((*read-eval* t)) (let ((*read-eval* nil)) (read s))))"),
            vec![Rebinding::Binding]
        );
    }

    #[test]
    fn does_not_defer_for_a_binding_with_no_reader_inside_a_definition() {
        assert_eq!(
            kinds("(defun f (x) (let ((*read-eval* t)) (compute x)))"),
            vec![Rebinding::Binding]
        );
    }

    // --- near misses ------------------------------------------------------

    #[test]
    fn does_not_flag_the_nil_guard() {
        assert!(kinds("(let ((*read-eval* nil)) (read s))").is_empty());
        assert!(kinds("(setf *read-eval* nil)").is_empty());
    }

    #[test]
    fn does_not_flag_another_special() {
        assert!(kinds("(setf *print-base* t)").is_empty());
        assert!(kinds("(let ((*print-pretty* t)) (print x))").is_empty());
    }

    #[test]
    fn does_not_flag_a_non_literal_value() {
        assert!(kinds("(setf *read-eval* saved)").is_empty());
        assert!(kinds("(let ((*read-eval* previous)) (read s))").is_empty());
    }

    #[test]
    fn does_not_flag_a_malformed_form() {
        assert!(kinds("(setf *read-eval*)").is_empty());
        assert!(kinds("(let)").is_empty());
    }

    // --- quote and string contexts ---------------------------------------

    #[test]
    fn does_not_flag_a_quoted_form() {
        assert!(kinds("'(setf *read-eval* t)").is_empty());
        assert!(kinds("'(progn (setf *read-eval* t))").is_empty());
        assert!(kinds("(quote (setf *read-eval* t))").is_empty());
        assert!(kinds("`(setf *read-eval* t)").is_empty());
        assert!(kinds("'(a ,(setf *read-eval* t))").is_empty());
    }

    #[test]
    fn flags_an_unquoted_form_inside_a_backquote() {
        assert_eq!(
            kinds("`(a ,(setf *read-eval* t))"),
            vec![Rebinding::Assignment]
        );
    }

    #[test]
    fn does_not_flag_text_inside_a_string_literal() {
        assert!(kinds(r#"(log-it "(setf *read-eval* t)")"#).is_empty());
    }
}
