//! Common Lisp redundant-`the` detection: a `(the t form)` type declaration.
//!
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
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};
use serde_json::{Value, json};

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

impl TheRedundancy {
    /// This redundancy's tag, for the report's `kind` column.
    ///
    /// The two are different facts — vacuous for every form there is, versus
    /// already satisfied by this one — and a consumer filtering on one of them
    /// is asking a real question.
    #[must_use]
    pub const fn tag(&self) -> &'static str {
        match self {
            Self::Vacuous => "vacuous",
            Self::AlreadySatisfied(_) => "already-satisfied",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RedundantTheItem {
    /// The span of the whole `(the TYPE form)` form.
    pub span: ByteSpan,
    /// The 1-based line the form starts on.
    pub line: usize,
    /// The span of the inner form (for reconstructing the fix).
    ///
    /// Published rather than kept internal: the old report emitted it, and a
    /// consumer that wants to apply the rewrite itself needs the extent of the
    /// text that survives it.
    pub form_span: ByteSpan,
    pub redundancy: TheRedundancy,
}

impl Finding for RedundantTheItem {
    fn kind(&self) -> &'static str {
        self.redundancy.tag()
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    /// The old text row carried nothing past the path and offset, and the
    /// redundancy is already the leading `kind` column.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![(
            "form_span",
            json!({
                "start": self.form_span.start().get(),
                "end": self.form_span.end().get(),
            }),
        )]
    }

    /// The same sentence the `redundant-the` lint rule writes, so a SARIF or
    /// JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        match &self.redundancy {
            TheRedundancy::Vacuous => {
                "(the t form) is a vacuous type declaration; it is just form".to_owned()
            }
            TheRedundancy::AlreadySatisfied(name) => {
                format!("(the {name} form) asserts a type the form already has; it is just form")
            }
        }
    }
}

/// Whether `form` provably already has the type `value_type` asserts.
///
/// The standalone `inspect redundant-the` command has no semantic tables to
/// consult, so it passes [`never`] and keeps reading `t` only. The lint suite
/// passes a test backed by the type context, so it also sees
/// `(the integer (length xs))` — an assertion just as empty, spelled in a way
/// the reader alone cannot recognize.
pub type IsAssertedTypeAlreadyKnown<'a> = &'a dyn Fn(&ExpressionView, &ExpressionView) -> bool;

/// The [`IsAssertedTypeAlreadyKnown`] of a caller with no type context.
const fn never(_: &ExpressionView, _: &ExpressionView) -> bool {
    false
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_the(
    view: &ExpressionView,
    source: &str,
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
        span: view.span,
        line: line_of(source, view.span.start().get()),
        form_span: form.span,
        redundancy,
    });
}

/// Collects every `(the t form)` in one file, with the number of `the` forms
/// scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no vacuous `the` here" for Common Lisp
/// and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_redundant_the_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<RedundantTheItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("the_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut the_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_the(
                subview,
                source,
                &never,
                &mut the_form_count,
                &mut violations,
            );
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("the_form_count", json!(the_form_count))],
    ))
}

fn line_of(source: &str, offset: usize) -> usize {
    1 + source
        .get(..offset.min(source.len()))
        .unwrap_or(source)
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<RedundantTheItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_redundant_the_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build redundant the report")
    }

    /// The `(the_form_count, violations)` pair the report is built from.
    fn thes(input: &str) -> (u64, Vec<RedundantTheItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "the_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("the_form_count in the summary");
        (count, report.findings)
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

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(the t x)", Dialect::Clojure).expect("parse");
        let report = build_redundant_the_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build redundant the report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("the_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(the fixnum x)").dialect_modelled);
    }

    /// The inner form's span is the one extra field the old report published,
    /// and it still crosses.
    #[test]
    fn a_finding_carries_its_line_its_kind_and_the_inner_form_span() {
        let source = "(defun f ()\n  (the t (g x)))\n";
        let report = report(source);
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "vacuous");
        assert!(finding.text_columns().is_empty());
        let span = finding.form_span;
        assert_eq!(&source[span.start().get()..span.end().get()], "(g x)");
        assert_eq!(
            finding.json_fields(),
            vec![(
                "form_span",
                json!({ "start": span.start().get(), "end": span.end().get() })
            )]
        );
    }

    #[test]
    fn the_summary_counts_every_the_scanned_not_only_the_flagged_ones() {
        let report = report("(the t x)\n(the fixnum y)\n(the t z)\n");
        assert_eq!(report.summary, vec![("the_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 2);
    }
}
