//! `racket-begin0-single-form`: a `(begin0 ⟨expression⟩)` whose body holds
//! exactly one form.
//!
//! `begin0` exists to evaluate a sequence and return the *first* result rather
//! than the last. Over one form there is no sequence and no distinction to
//! draw: `(begin0 e)` evaluates `e` in the same place and returns its values,
//! introducing no binding and no control context. It is pure wrapping.
//!
//! Verified on Racket v9.2: `(displayln (begin0 42))` prints `42`, and the
//! compiler says nothing about the redundant wrapper.
//!
//! # Why this is not `scheme-begin-single-form`
//!
//! `paredit-feature-lint-scheme-idiom`'s `scheme-begin-single-form` anchors on
//! the head `begin` and nothing else. Its `HeadFilter::Heads(&["begin"])` means
//! the dispatcher never offers it a `begin0` node at all, so `begin0` is not
//! merely unreported by that rule — it is unreachable from it. The two forms
//! also differ in what they return over a *multi*-form body, which is why they
//! cannot share an implementation even though they agree on the single-form
//! case.
//!
//! `begin0` is Racket and R6RS; R7RS has no such form. Scope is Racket only,
//! because the Scheme package already owns the Scheme surface and a
//! `Dialect::Scheme` file reaching `begin0` is using an implementation
//! extension this package has not audited.
//!
//! Two shapes are deliberately not flagged:
//!
//! - **`(begin0)`**, which is a syntax error in Racket rather than an idiom.
//! - **A `begin0` with two or more forms**, which is `begin0` doing its job.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use serde_json::{Value, json};

use crate::support::{heads_with, is_inert_at};

/// The head this rule anchors on, shared with its `HeadFilter`.
pub const HEAD: &str = "begin0";

/// The dialects this rule models.
pub const DIALECTS: [Dialect; 1] = [Dialect::Racket];

#[derive(Debug, Clone)]
pub struct Begin0SingleFormItem {
    /// The span of the whole `(begin0 …)` form, which a fix replaces.
    pub span: ByteSpan,
    /// The span of the sole body form, whose source a fix copies verbatim.
    ///
    /// Includes any reader prefix the form carries, so `(begin0 'x)` fixes to
    /// `'x` rather than to `x`.
    pub inner_span: ByteSpan,
}

impl Finding for Begin0SingleFormItem {
    fn kind(&self) -> &'static str {
        "racket-begin0-single-form"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec!["body_form_count=1".to_owned()]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("body_form_count", json!(1))]
    }

    fn message(&self) -> String {
        MESSAGE.to_owned()
    }
}

/// The one sentence both the report and the lint rule write.
pub const MESSAGE: &str = "begin0 wraps a single expression; with nothing to sequence it is equivalent to that expression";

/// Examines one node, which the caller has already narrowed to a `begin0` head.
pub fn examine_begin0(
    tree: &SyntaxTree,
    view: &ExpressionView,
    begin0_form_count: &mut usize,
    violations: &mut Vec<Begin0SingleFormItem>,
) {
    if !heads_with(view, HEAD) {
        return;
    }
    *begin0_form_count += 1;

    // children: [begin0, body…]. Exactly one body form is the whole subject.
    let [_, only] = view.children.as_slice() else {
        return;
    };

    // Last, and only for a node that would otherwise be reported: this is the
    // only part of the rule whose cost is not node-local.
    if is_inert_at(tree, view.span) {
        return;
    }

    violations.push(Begin0SingleFormItem {
        span: view.span,
        inner_span: only.span,
    });
}

/// Collects every single-form `begin0` in one file, with the number of `begin0`
/// forms scanned as the denominator beside them.
pub fn build_begin0_single_form_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<Begin0SingleFormItem>> {
    let modelled = DIALECTS.contains(&dialect);
    let mut begin0_form_count = 0;
    let mut violations = Vec::new();

    if modelled {
        let root = tree.root_view();
        let mut stack: Vec<&ExpressionView> = root.children.iter().rev().collect();
        while let Some(view) = stack.pop() {
            examine_begin0(tree, view, &mut begin0_form_count, &mut violations);
            stack.extend(view.children.iter().rev());
        }
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        modelled,
        tree.source(),
        violations,
        vec![("begin0_form_count", json!(begin0_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<Begin0SingleFormItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Racket).expect("parse input");
        build_begin0_single_form_report(Path::new("main.rkt"), Dialect::Racket, &tree)
            .expect("build report")
    }

    fn findings(input: &str) -> Vec<Begin0SingleFormItem> {
        report(input).findings
    }

    fn scanned(input: &str) -> u64 {
        report(input)
            .summary
            .iter()
            .find(|(name, _)| *name == "begin0_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("begin0_form_count in the summary")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_a_begin0_wrapping_a_single_call() {
        let source = "(begin0 (compute))";
        let found = findings(source);
        assert_eq!(found.len(), 1);
        assert_eq!(slice(source, found[0].inner_span), "(compute)");
    }

    #[test]
    fn flags_a_begin0_wrapping_a_single_atom() {
        assert_eq!(findings("(begin0 x)").len(), 1);
    }

    #[test]
    fn does_not_flag_a_begin0_doing_its_job() {
        assert_eq!(scanned("(begin0 (result) (cleanup))"), 1);
        assert!(findings("(begin0 (result) (cleanup))").is_empty());
    }

    #[test]
    fn does_not_flag_an_empty_begin0() {
        assert_eq!(scanned("(begin0)"), 1);
        assert!(findings("(begin0)").is_empty());
    }

    /// The sibling package's rule anchors on `begin`, which this rule must not
    /// answer for, and vice versa.
    #[test]
    fn does_not_flag_a_plain_begin() {
        assert_eq!(scanned("(begin x)"), 0);
        assert!(findings("(begin x)").is_empty());
    }

    #[test]
    fn does_not_case_fold_the_head() {
        assert_eq!(scanned("(BEGIN0 x)"), 0);
    }

    #[test]
    fn does_not_flag_a_qualified_head_that_merely_ends_in_begin0() {
        assert_eq!(scanned("(racket:begin0 x)"), 0);
    }

    #[test]
    fn does_not_flag_a_quoted_begin0_shape() {
        assert!(findings("'(begin0 x)").is_empty());
        assert!(findings("(quote (begin0 x))").is_empty());
        assert!(findings("`(a (begin0 x))").is_empty());
    }

    #[test]
    fn flags_an_unquoted_begin0_inside_a_quasiquote() {
        assert_eq!(findings("`(a ,(begin0 (f x)))").len(), 1);
    }

    #[test]
    fn does_not_flag_a_vector_constant_that_looks_like_a_begin0_call() {
        assert_eq!(scanned("#(begin0 x)"), 0);
    }

    #[test]
    fn does_not_flag_a_begin0_inside_a_macro_template() {
        assert!(findings("(define-syntax m (syntax-rules () ((_ e) (begin0 e))))").is_empty());
    }

    #[test]
    fn finds_a_nested_begin0() {
        assert_eq!(findings("(define (f) (begin0 (begin0 x)))").len(), 2);
    }

    #[test]
    fn a_bracketed_begin0_is_found() {
        assert_eq!(findings("(cond [#t [begin0 x]])").len(), 1);
    }

    #[test]
    fn the_inner_span_keeps_a_reader_prefix_with_its_form() {
        let source = "(begin0 'x)";
        assert_eq!(slice(source, findings(source)[0].inner_span), "'x");
    }

    #[test]
    fn the_summary_counts_every_begin0_scanned() {
        let source = "(begin0 x)\n(begin0 a b)\n(begin0)\n";
        assert_eq!(scanned(source), 3);
        assert_eq!(findings(source).len(), 1);
    }

    #[test]
    fn the_same_bytes_are_flagged_as_racket_and_unmodelled_elsewhere() {
        let source = "(begin0 x)\n";
        assert_eq!(findings(source).len(), 1);
        for dialect in [Dialect::Scheme, Dialect::CommonLisp, Dialect::Clojure] {
            let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
            let report = build_begin0_single_form_report(Path::new("f.scm"), dialect, &tree)
                .expect("build report");
            assert!(!report.dialect_modelled, "{dialect:?}");
            assert!(report.findings.is_empty(), "{dialect:?}");
        }
    }

    #[test]
    fn a_finding_carries_its_line_and_its_fields() {
        let report = report("#lang racket\n(begin0 (compute))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "racket-begin0-single-form");
        assert_eq!(finding.json_fields(), vec![("body_form_count", json!(1))]);
        assert_eq!(finding.message(), MESSAGE);
    }
}
