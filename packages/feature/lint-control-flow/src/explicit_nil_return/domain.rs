//! Common Lisp explicit-nil-return detection: a `return` or `return-from` whose
//! result form is the literal `nil`. The result argument of both defaults to
//! `nil`, so `(return nil)` is exactly `(return)` and `(return-from foo nil)` is
//! exactly `(return-from foo)` — same block exited, same `nil` produced.
//! Dropping the redundant `nil` states the unit return the way the operators
//! were designed to express it.
//!
//! The two operators are matched by their own arities, and the distinction
//! matters:
//!
//!   - `(return nil)` — exactly two elements; the `nil` is the result.
//!   - `(return-from foo nil)` — exactly three elements; the `nil` is the
//!     result and `foo` is the block name, which is preserved.
//!
//! `(return-from nil)` is *not* matched: there the `nil` is the block name (the
//! implicit `nil` block), not a result. A non-`nil` result, a reader-conditional
//! operand, and any other arity are left alone.
//!
//! The fix drops the redundant `nil`, copying the operator (and, for
//! `return-from`, the block name) from its exact source, so the rule is
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

/// Whether `view` is the bare `nil` literal (case-insensitive, no reader
/// prefixes so a quoted/`,`-prefixed `nil` is excluded).
fn is_nil_literal(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|t| t.eq_ignore_ascii_case("nil"))
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct ExplicitNilReturnItem {
    /// The span of the whole `(return nil)` / `(return-from b nil)` form.
    pub span: ByteSpan,
    /// The 1-based line the form starts on.
    pub line: usize,
    /// The span of the `return`/`return-from` head symbol (exact source).
    ///
    /// The rewrite's input, not the report's: the lint rule copies the operator
    /// from it, and the command never prints it.
    pub head_span: ByteSpan,
    /// The span of the block name (`return-from` only; `None` for `return`).
    ///
    /// The rewrite's input, not the report's: the lint rule copies the block
    /// name from it, and the command never prints it.
    pub block_span: Option<ByteSpan>,
    /// The canonical operator name (`return`/`return-from`), for the message.
    pub operator: &'static str,
}

impl Finding for ExplicitNilReturnItem {
    /// The operator, so `return` and `return-from` are separable without
    /// parsing JSON. They are two different edits — one drops an operand, the
    /// other drops an operand and keeps a block name — and a consumer filtering
    /// on one of them is asking a real question.
    fn kind(&self) -> &'static str {
        self.operator
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    /// Empty: the operator is the only thing the old text row carried beyond
    /// the path and offset, and it now leads every row as the `kind`.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("operator", json!(self.operator))]
    }

    /// The same sentence the `explicit-nil-return` lint rule writes, so a SARIF
    /// or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!(
            "{} nil result is the default; drop the redundant nil",
            self.operator
        )
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_return(
    view: &ExpressionView,
    source: &str,
    return_form_count: &mut usize,
    violations: &mut Vec<ExplicitNilReturnItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let is_return = head.eq_ignore_ascii_case("return");
    let is_return_from = head.eq_ignore_ascii_case("return-from");
    if !is_return && !is_return_from {
        return;
    }
    *return_form_count += 1;

    let head_span = view.children[0].span;
    if is_return {
        // (return nil): exactly the result operand.
        if view.children.len() != 2 {
            return;
        }
        let result = &view.children[1];
        if is_reader_conditional(result) || !is_nil_literal(result) {
            return;
        }
        violations.push(ExplicitNilReturnItem {
            span: view.span,
            line: line_of(source, view.span.start().get()),
            head_span,
            block_span: None,
            operator: "return",
        });
    } else {
        // (return-from block nil): block name plus the result operand.
        if view.children.len() != 3 {
            return;
        }
        let block = &view.children[1];
        let result = &view.children[2];
        if is_reader_conditional(block) || is_reader_conditional(result) || !is_nil_literal(result)
        {
            return;
        }
        violations.push(ExplicitNilReturnItem {
            span: view.span,
            line: line_of(source, view.span.start().get()),
            head_span,
            block_span: Some(block.span),
            operator: "return-from",
        });
    }
}

/// Collects every `return`/`return-from` with an explicit `nil` result in one
/// file, with the number of `return`/`return-from` forms scanned as the
/// denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no redundant nil result here" for Common
/// Lisp and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_explicit_nil_return_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<ExplicitNilReturnItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("return_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut return_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_return(subview, source, &mut return_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("return_form_count", json!(return_form_count))],
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

    fn report(input: &str) -> FileFindings<ExplicitNilReturnItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_explicit_nil_return_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build explicit nil return report")
    }

    /// The `(return_form_count, violations)` pair the report is built from.
    fn returns(input: &str) -> (u64, Vec<ExplicitNilReturnItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "return_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("return_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_return_nil() {
        let source = "(return nil)";
        let (count, violations) = returns(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "return");
        assert!(violations[0].block_span.is_none());
    }

    #[test]
    fn flags_return_from_nil() {
        let source = "(return-from search nil)";
        let (_, violations) = returns(source);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "return-from");
        let block = violations[0].block_span.expect("block span");
        assert_eq!(slice(source, block), "search");
    }

    #[test]
    fn does_not_flag_a_non_nil_result() {
        let (count, violations) = returns("(return 5)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_return_from_without_result() {
        // (return-from foo) already has no result to drop.
        let (count, violations) = returns("(return-from foo)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_return_from_nil_block_only() {
        // (return-from nil) exits the nil block; the nil is the block name.
        let (_, violations) = returns("(return-from nil)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_bare_return() {
        let (count, violations) = returns("(return)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head_and_nil() {
        let (_, violations) = returns("(RETURN NIL)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "return");
    }

    #[test]
    fn finds_a_nested_return() {
        let (_, violations) = returns("(loop (when done (return nil)))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(return nil)", Dialect::Clojure).expect("parse");
        let report =
            build_explicit_nil_return_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build explicit nil return report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("return_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(return 5)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_operator() {
        let report = report("(loop\n  (return nil))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "return");
        assert_eq!(finding.json_fields(), vec![("operator", json!("return"))]);
        assert!(finding.text_columns().is_empty());
        assert_eq!(
            finding.message(),
            "return nil result is the default; drop the redundant nil"
        );
    }

    #[test]
    fn the_summary_counts_every_return_scanned_not_only_the_flagged_ones() {
        let report = report("(return nil)\n(return 5)\n(return-from foo nil)\n");
        assert_eq!(report.summary, vec![("return_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 2);
    }
}
