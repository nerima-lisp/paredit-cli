//! `scheme-begin-single-form` detection: a `(begin ⟨expression⟩)` whose body
//! holds exactly one form.
//!
//! R7RS 4.2.3 gives `begin` one job — "the expressions are evaluated
//! sequentially from left to right, and the values of the last expression are
//! returned" — so a `begin` over a single expression returns that
//! expression's values, evaluated in the same place, with no binding or
//! control context introduced. It is pure wrapping.
//!
//! The same holds where `begin` *splices* rather than sequences. R7RS 5.6.1
//! and Racket's `begin` both flatten a `begin` appearing directly in a body or
//! at top level into the surrounding definition context, which is exactly what
//! makes `(begin ⟨definition⟩)` equal to that definition there too. So the
//! definition-context case, which is the one worth being suspicious of, is not
//! an exception: it is the reason the rule is safe for one-form bodies of
//! either kind.
//!
//! Two shapes are deliberately *not* flagged:
//!
//! - **A `begin` with zero or two or more forms.** Zero is only legal in a
//!   definition context at all, and there it means "no definitions"; two or
//!   more is `begin` doing its job.
//! - **A `begin` that is a direct child of `define-library`/`library`.**
//!   There `begin` is not the expression operator but a *library declaration*
//!   (R7RS 5.6.1), and the surrounding form admits only declarations. Removing
//!   it does not simplify the library, it makes it ill-formed.
//!
//! Scope: Scheme and Racket. Common Lisp's `progn` counterpart is
//! `redundant-progn` in `paredit-feature-lint-control-flow`, which is
//! `COMMON_LISP_ONLY` and anchors on a head this dialect does not have.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, ReaderPrefix, SyntaxTree};
use serde_json::{Value, json};

use crate::support::{heads_with, node_context};

/// The head this rule anchors on, shared with its `HeadFilter`.
pub const HEAD: &str = "begin";

/// The forms whose direct children named `begin` are library declarations
/// rather than expressions: R7RS `define-library`, and the R6RS `library`
/// spelling the syntax crate's own operator table already treats as its
/// synonym (`packages/core/syntax/src/scheme/operator/table.rs`).
const DECLARATION_PARENTS: [&str; 2] = ["define-library", "library"];

/// The dialects this rule models.
pub const DIALECTS: [Dialect; 2] = [Dialect::Scheme, Dialect::Racket];

#[derive(Debug, Clone)]
pub struct BeginSingleFormItem {
    /// The span of the whole `(begin …)` form, which a fix replaces.
    pub span: ByteSpan,
    /// The span of the sole body form, whose source a fix copies verbatim.
    ///
    /// Includes any reader prefix the form carries, so `(begin 'x)` fixes to
    /// `'x` rather than to `x`.
    pub inner_span: ByteSpan,
}

impl Finding for BeginSingleFormItem {
    fn kind(&self) -> &'static str {
        "scheme-begin-single-form"
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

/// The one sentence both the standalone report and the lint rule write, so a
/// SARIF or JUnit consumer reading both sees one finding described one way.
pub const MESSAGE: &str = "begin wraps a single expression; it is equivalent to that expression";

/// Examines one node, which the caller has already narrowed to a `begin` head
/// (or is about to have this function narrow for it).
///
/// `tree` is needed because neither question this rule has to answer is a
/// property of the node: whether the form is code or quoted data, and what it
/// is nested directly inside.
pub fn examine_begin(
    tree: &SyntaxTree,
    view: &ExpressionView,
    begin_form_count: &mut usize,
    violations: &mut Vec<BeginSingleFormItem>,
) {
    if !heads_with(view, HEAD) {
        return;
    }
    *begin_form_count += 1;

    // children: [begin, body…]. Exactly one body form is the whole subject.
    let [_, only] = view.children.as_slice() else {
        return;
    };

    // Last, and in one descent: this is the only part of the rule whose cost
    // is not node-local. See `crate::support::node_context`.
    let context = node_context(tree, view.span);
    if context.unevaluated {
        return;
    }
    // An unquote with no quasiquote above it is not evaluable Scheme, so a
    // `(begin ,form)` at quasi depth zero is a *matcher clause pattern*, not a
    // form — Guile's `language/ecmascript/compile-tree-il.scm` spells its
    // pattern-matching clauses `((begin ,form) (comp form e))`, and reading
    // that as code was this rule's only false positive over its corpus.
    let unquoted_body = only.reader_prefixes.iter().any(|prefix| {
        matches!(
            prefix,
            ReaderPrefix::Unquote | ReaderPrefix::UnquoteSplicing
        )
    });
    if unquoted_body && context.quasi_depth == 0 {
        return;
    }
    if context
        .parent_head
        .is_some_and(|head| DECLARATION_PARENTS.contains(&head.as_str()))
    {
        return;
    }

    violations.push(BeginSingleFormItem {
        span: view.span,
        inner_span: only.span,
    });
}

/// Collects every single-form `begin` in one file, with the number of `begin`
/// forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no single-form begin here" for Scheme
/// and "nothing was looked for" for Common Lisp, and the two read identically
/// without the flag.
pub fn build_begin_single_form_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<BeginSingleFormItem>> {
    if !DIALECTS.contains(&dialect) {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("begin_form_count", json!(0))],
        ));
    }

    let mut begin_form_count = 0;
    let mut violations = Vec::new();
    let root = tree.root_view();
    let mut stack: Vec<&ExpressionView> = root.children.iter().rev().collect();
    while let Some(view) = stack.pop() {
        examine_begin(tree, view, &mut begin_form_count, &mut violations);
        stack.extend(view.children.iter().rev());
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("begin_form_count", json!(begin_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<BeginSingleFormItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Scheme).expect("parse input");
        build_begin_single_form_report(Path::new("test.scm"), Dialect::Scheme, &tree)
            .expect("build begin-single-form report")
    }

    fn findings(input: &str) -> Vec<BeginSingleFormItem> {
        report(input).findings
    }

    fn scanned(input: &str) -> u64 {
        report(input)
            .summary
            .iter()
            .find(|(name, _)| *name == "begin_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("begin_form_count in the summary")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_a_begin_wrapping_a_single_call() {
        let source = "(begin (display x))";
        let found = findings(source);
        assert_eq!(found.len(), 1);
        assert_eq!(slice(source, found[0].inner_span), "(display x)");
    }

    #[test]
    fn flags_a_begin_wrapping_a_single_atom() {
        assert_eq!(findings("(begin x)").len(), 1);
    }

    #[test]
    fn flags_a_begin_wrapping_a_single_definition() {
        // R7RS 5.6.1: a `begin` in a body or at top level splices, so this is
        // exactly the definition it wraps.
        assert_eq!(findings("(begin (define x 1))").len(), 1);
    }

    #[test]
    fn does_not_flag_a_begin_with_two_forms() {
        assert_eq!(scanned("(begin (setup) (run))"), 1);
        assert!(findings("(begin (setup) (run))").is_empty());
    }

    #[test]
    fn does_not_flag_an_empty_begin() {
        assert_eq!(scanned("(begin)"), 1);
        assert!(findings("(begin)").is_empty());
    }

    #[test]
    fn does_not_flag_a_library_declaration_begin() {
        // R7RS 5.6.1: inside `define-library`, `begin` is a declaration and a
        // bare `(define x 1)` is not one. Unwrapping would break the library.
        assert!(findings("(define-library (a b) (begin (define x 1)))").is_empty());
        assert!(findings("(library (a b) (begin (define x 1)))").is_empty());
    }

    #[test]
    fn flags_a_begin_nested_one_level_below_a_library_declaration_begin() {
        // Only the *direct* child of `define-library` is a declaration; a
        // `begin` inside that one is an ordinary body form again.
        let found = findings("(define-library (a) (begin (begin (define x 1))))");
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn does_not_flag_a_quoted_begin_shape() {
        assert!(findings("'(begin x)").is_empty());
        assert!(findings("(quote (begin x))").is_empty());
        assert!(findings("`(a (begin x))").is_empty());
    }

    #[test]
    fn flags_an_unquoted_begin_inside_a_quasiquote() {
        assert_eq!(findings("`(a ,(begin x))").len(), 1);
    }

    #[test]
    fn does_not_flag_a_vector_constant_that_looks_like_a_begin_call() {
        // `#(begin x)` parses as a paren-delimited list with a `HashLiteral`
        // prefix, so the lint engine's own head index would offer it to this
        // rule. It is a self-evaluating vector (R7RS 4.1.2), not a form.
        assert_eq!(scanned("#(begin x)"), 0);
        assert!(findings("#(begin x)").is_empty());
    }

    #[test]
    fn does_not_case_fold_the_head() {
        // Scheme is case sensitive (R7RS 2.1): `BEGIN` is a different name.
        assert_eq!(scanned("(BEGIN x)"), 0);
        assert!(findings("(BEGIN x)").is_empty());
    }

    #[test]
    fn does_not_flag_a_qualified_head_that_merely_ends_in_begin() {
        assert_eq!(scanned("(srfi:begin x)"), 0);
    }

    /// Reduced from `language/ecmascript/compile-tree-il.scm:344`, the only
    /// false positive this rule produced over the Guile corpus. The clause
    /// head `(begin ,form)` is a *pattern* the matcher compares against, and
    /// `,form` outside any quasiquote is not evaluable Scheme at all.
    #[test]
    fn does_not_flag_a_matcher_clause_pattern() {
        let source = "(pmatch form\n  ((begin) (-> (void)))\n  ((begin ,form) (comp form e))\n  ((begin . ,forms) `(begin ,@(map (lambda (x) (comp x e)) forms))))";
        assert!(findings(source).is_empty());
    }

    /// The narrow version of the same check: an unquote *inside* a quasiquote
    /// is an ordinary splice and the `begin` around it is real code, so the
    /// suppression must not swallow it.
    #[test]
    fn still_flags_an_unquoted_body_inside_a_quasiquote() {
        assert_eq!(findings("`(a ,(begin (f x)))").len(), 1);
    }

    #[test]
    fn finds_a_nested_begin() {
        assert_eq!(findings("(define (f) (begin (begin x)))").len(), 2);
    }

    #[test]
    fn a_bracketed_begin_is_found() {
        assert_eq!(findings("(cond [#t [begin x]])").len(), 1);
    }

    #[test]
    fn the_summary_counts_every_begin_scanned_not_only_the_flagged_ones() {
        assert_eq!(scanned("(begin x)\n(begin a b)\n(begin)\n"), 3);
        assert_eq!(findings("(begin x)\n(begin a b)\n(begin)\n").len(), 1);
    }

    #[test]
    fn a_dialect_this_rule_does_not_model_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(begin x)", Dialect::CommonLisp).expect("parse");
        let report =
            build_begin_single_form_report(Path::new("a.lisp"), Dialect::CommonLisp, &tree)
                .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("begin_form_count", json!(0))]);
    }

    #[test]
    fn racket_is_modelled_too() {
        let tree = SyntaxTree::parse_with_dialect("(begin x)", Dialect::Racket).expect("parse");
        let report = build_begin_single_form_report(Path::new("a.rkt"), Dialect::Racket, &tree)
            .expect("build report");
        assert!(report.dialect_modelled);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_body_form_count() {
        let report = report("(define (f)\n  (begin (compute)))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "scheme-begin-single-form");
        assert_eq!(finding.json_fields(), vec![("body_form_count", json!(1))]);
        assert_eq!(finding.text_columns(), vec!["body_form_count=1".to_owned()]);
        assert_eq!(finding.message(), MESSAGE);
    }

    #[test]
    fn the_inner_span_keeps_a_reader_prefix_with_its_form() {
        let source = "(begin 'x)";
        let found = findings(source);
        assert_eq!(slice(source, found[0].inner_span), "'x");
    }
}
