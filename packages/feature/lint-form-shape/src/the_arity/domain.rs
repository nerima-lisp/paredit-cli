//! Common Lisp `the`-arity detection: a `the` special form with the wrong
//! number of arguments. `the` is `(the value-type form)` — it takes exactly
//! two arguments, a type specifier and a form. Fewer (`(the fixnum)`) or more
//! (`(the fixnum x y)`) is a program error, caught at compile time rather than
//! by the reader.
//!
//! Forms whose written arity may differ from their evaluated arity are skipped
//! to avoid false positives: a quoted/quasiquoted `the` (data or a template),
//! and any `the` with a child carrying a `#+`/`#-` reader conditional or a
//! splicing unquote (`,@`) — e.g. `(the #+sbcl fixnum #-sbcl integer x)` is a
//! valid feature-portable declaration whose written three-token shape is not a
//! real arity error.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionView, Path as SexprPath, ReaderPrefix, SyntaxTree,
};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};
use serde_json::{Value, json};

const EXPECTED_ARGUMENTS: usize = 2;

/// Whether a child form can change how many argument forms actually reach the
/// evaluator, making a static arity count unreliable: a `,@` splice or Clojure
/// reader conditional (a prefix), or a Common Lisp `#+`/`#-` conditional (an
/// atom whose text begins `#+`/`#-`).
fn is_arity_ambiguous(view: &ExpressionView) -> bool {
    let ambiguous_prefix = view.reader_prefixes.iter().any(|prefix| {
        matches!(
            prefix,
            ReaderPrefix::ReaderConditional
                | ReaderPrefix::ReaderConditionalSplicing
                | ReaderPrefix::UnquoteSplicing
        )
    });
    ambiguous_prefix
        || atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct TheArityItem {
    /// The span of the whole `(the …)` form.
    pub span: ByteSpan,
    /// How many arguments were written, which is anything but two.
    pub argument_count: usize,
}

impl Finding for TheArityItem {
    /// One tag for every finding: what separates these is the argument count,
    /// and a count is a quantity rather than a class of defect. It stays in the
    /// columns and the JSON, where a consumer can compare it numerically.
    fn kind(&self) -> &'static str {
        "the-arity"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("arguments={}", self.argument_count)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("argument_count", json!(self.argument_count))]
    }

    /// The same sentence the `the-arity` lint rule writes, so a SARIF or JUnit
    /// consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!(
            "the takes exactly 2 arguments (a type and a form) but has {}",
            self.argument_count
        )
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_the(
    view: &ExpressionView,
    the_form_count: &mut usize,
    violations: &mut Vec<TheArityItem>,
) {
    if !list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("the")) {
        return;
    }
    // A quoted/quasiquoted/unquoted `the` is data or a template, not a call.
    if !view.reader_prefixes.is_empty() {
        return;
    }
    // A `#+`/`#-` or `,@` argument makes the written arity unreliable.
    if view.children.iter().skip(1).any(is_arity_ambiguous) {
        return;
    }
    *the_form_count += 1;

    let argument_count = view.children.len() - 1;
    if argument_count != EXPECTED_ARGUMENTS {
        violations.push(TheArityItem {
            span: view.span,
            argument_count,
        });
    }
}

/// Collects every misarity `the` form in one file, with the number of `the`
/// forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "every `the` here is well-formed" for
/// Common Lisp and "nothing was looked for" for Clojure, and the two read
/// identically without the flag.
pub fn build_the_arity_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<TheArityItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("the_form_count", json!(0))],
        ));
    }

    let mut the_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_the(subview, &mut the_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("the_form_count", json!(the_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<TheArityItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_the_arity_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build the arity report")
    }

    /// The `(the_form_count, violations)` pair the report is built from.
    fn violations(input: &str) -> (u64, Vec<TheArityItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "the_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("the_form_count in the summary");
        (count, report.findings)
    }

    #[test]
    fn flags_too_few_arguments() {
        let (the_form_count, items) = violations("(the fixnum)");
        assert_eq!(the_form_count, 1);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].argument_count, 1);
    }

    #[test]
    fn flags_too_many_arguments() {
        let (_, items) = violations("(the fixnum x y)");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].argument_count, 3);
    }

    #[test]
    fn does_not_flag_a_two_argument_the() {
        let (the_form_count, items) = violations("(the fixnum (+ a b))");
        assert_eq!(the_form_count, 1);
        assert!(items.is_empty());
    }

    #[test]
    fn skips_a_reader_conditional_type() {
        let (the_form_count, items) = violations("(the #+sbcl fixnum #-sbcl integer x)");
        assert_eq!(the_form_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn skips_a_quoted_the_form() {
        let (the_form_count, items) = violations("(list '(the fixnum))");
        assert_eq!(the_form_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn finds_a_the_nested_in_a_function_body() {
        let (the_form_count, items) = violations("(defun f (x) (the fixnum x x))");
        assert_eq!(the_form_count, 1);
        assert_eq!(items.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(the fixnum)", Dialect::Clojure).expect("parse input");
        let report = build_the_arity_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build the arity report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("the_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(the fixnum x)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_argument_count() {
        let report = report("(defun f (x)\n  (the fixnum x x))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "the-arity");
        assert_eq!(
            finding.json_fields(),
            vec![("argument_count", json!(3_usize))]
        );
        assert_eq!(finding.text_columns(), vec!["arguments=3".to_owned()]);
    }

    #[test]
    fn the_summary_counts_every_the_scanned_not_only_the_flagged_ones() {
        let report = report("(the fixnum)\n(the fixnum x)\n(the fixnum x y)\n");
        assert_eq!(report.summary, vec![("the_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 2);
    }
}
