//! Common Lisp redundant-`the` detection: a `(the t form)` type declaration.
//! `the` asserts that `form` yields values of the given type; the type `t`
//! matches every object, so `(the t form)` asserts nothing and simply returns
//! `form`'s values unchanged (`the` passes all values through, so this holds in
//! multiple-value contexts too). `(the t x)` is exactly `x`.
//!
//! `t` is only the *unconditionally* vacuous case. `(the integer (length xs))`
//! is just as empty a claim — `length` returns an integer whatever it is
//! given — but saying so needs a type context, so callers that have one pass
//! a second test (see [`IsAssertedTypeAlreadyKnown`]) and callers that do not
//! keep reading `t` alone.
//!
//! The reasoning is not circular: the type layer records a `the` form's
//! asserted type against *that form's* key and infers the inner form
//! independently, so asking what the inner form is never returns the
//! assertion being judged.
//!
//! Only the exact `(the TYPE form)` three-element shape is matched; a
//! reader-conditional operand is left alone (build-dependent), and a compound
//! or unmodelled specifier (`(integer 0 9)`, `fixnum`) is one the type layer
//! declines to name, so it stays silent rather than guessing.
//!
//! The fix replaces the whole form with the inner form's source, so the rule is
//! auto-fixable.
//!
//! Reuses the shared whole-tree walk from
//! [`crate::domain::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use crate::domain::view_query::{atom_text, for_each_subview, list_head};

/// Whether `view` is the bare `t` type specifier (no reader prefixes).
fn is_t_type(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case("t"))
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

/// Why the assertion says nothing.
///
/// An enum rather than a type-name string that is empty for the `t` case:
/// "vacuous for every form there is" and "already satisfied by this form" are
/// different facts, and a rewrite that dropped the distinction would leave the
/// message unable to say which one it found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TheRedundancy {
    /// `(the t form)`: `t` matches every object, so the claim is empty
    /// whatever the form is.
    Vacuous,
    /// `(the integer (length xs))`: a real type, which the form provably has
    /// already. Carries the asserted type's source spelling.
    AlreadySatisfied(String),
}

#[derive(Debug, Clone)]
pub struct RedundantTheItem {
    pub path: PathBuf,
    /// The span of the whole `(the TYPE form)` form.
    pub span: ByteSpan,
    /// The span of the inner form (for reconstructing the fix).
    pub form_span: ByteSpan,
    pub redundancy: TheRedundancy,
}

#[derive(Debug)]
pub struct RedundantTheSummary {
    pub the_form_count: usize,
    pub violations: Vec<RedundantTheItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct RedundantThePolicyOptions {
    fail_on_violation: bool,
}

impl RedundantThePolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct RedundantThePolicy {
    pub fail_on_violation: bool,
    pub the_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Whether `form` provably already has the type `value_type` asserts.
///
/// The standalone `inspect redundant-the` command has no semantic tables to
/// consult, so it passes [`never`] and keeps reading `t` only. The lint suite
/// passes a test backed by the type context, so it also sees
/// `(the integer (length xs))` — an assertion just as empty, spelled in a way
/// the reader alone cannot recognize.
pub(crate) type IsAssertedTypeAlreadyKnown<'a> =
    &'a dyn Fn(&ExpressionView, &ExpressionView) -> bool;

/// The [`IsAssertedTypeAlreadyKnown`] of a caller with no type context.
fn never(_: &ExpressionView, _: &ExpressionView) -> bool {
    false
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine_the(
    view: &ExpressionView,
    path: &Path,
    already_known: IsAssertedTypeAlreadyKnown<'_>,
    the_form_count: &mut usize,
    violations: &mut Vec<RedundantTheItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("the") {
        return;
    }
    *the_form_count += 1;

    // children: [the, value-type, form] — require exactly this shape.
    if view.children.len() != 3 {
        return;
    }
    let value_type = &view.children[1];
    let form = &view.children[2];
    if is_reader_conditional(value_type) || is_reader_conditional(form) {
        return;
    }

    // `t` first, so a form that is both keeps the message that needs no type
    // context to explain.
    let redundancy = if is_t_type(value_type) {
        TheRedundancy::Vacuous
    } else if already_known(value_type, form) {
        let Some(name) = atom_text(value_type) else {
            return;
        };
        TheRedundancy::AlreadySatisfied(name.to_owned())
    } else {
        return;
    };

    violations.push(RedundantTheItem {
        path: path.to_path_buf(),
        span: view.span,
        form_span: form.span,
        redundancy,
    });
}

/// Collects every `(the t form)` across a whole file, along with the total
/// number of `the` forms scanned.
pub fn collect_redundant_thes(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<RedundantTheItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut the_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_the(subview, path, &never, &mut the_form_count, &mut violations);
        });
    }
    Ok((the_form_count, violations))
}

pub fn summarize_redundant_thes(
    the_form_count: usize,
    violations: Vec<RedundantTheItem>,
) -> RedundantTheSummary {
    RedundantTheSummary {
        the_form_count,
        violations,
    }
}

pub fn evaluate_redundant_the_policy(
    options: RedundantThePolicyOptions,
    summary: &RedundantTheSummary,
) -> RedundantThePolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    RedundantThePolicy {
        fail_on_violation: options.fail_on_violation(),
        the_form_count: summary.the_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thes(input: &str) -> (usize, Vec<RedundantTheItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_redundant_thes(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect redundant the")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_the_t() {
        let source = "(the t (compute))";
        let (count, violations) = thes(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].form_span), "(compute)");
    }

    #[test]
    fn flags_the_t_on_a_symbol() {
        let source = "(the t x)";
        let (_, violations) = thes(source);
        assert_eq!(slice(source, violations[0].form_span), "x");
    }

    #[test]
    fn does_not_flag_a_specific_type() {
        let (count, violations) = thes("(the fixnum x)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_compound_type() {
        let (_, violations) = thes("(the (values integer) x)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head_and_type() {
        let (_, violations) = thes("(THE T x)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_wrong_arity() {
        // (the t) and (the t a b) are the-arity's concern, not this rule.
        assert!(thes("(the t)").1.is_empty());
        assert!(thes("(the t a b)").1.is_empty());
    }

    #[test]
    fn finds_a_nested_the() {
        let (_, violations) = thes("(defun f (x) (the t (g x)))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(the t x)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_redundant_thes(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect redundant the");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = thes("(the t x)");
        let summary = summarize_redundant_thes(count, items);

        let quiet = evaluate_redundant_the_policy(RedundantThePolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_redundant_the_policy(RedundantThePolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
