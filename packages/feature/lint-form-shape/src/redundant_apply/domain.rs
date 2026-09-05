//! Common Lisp redundant-`apply` detection: `(apply #'FN (list ARGS…))` where
//! the function is a sharp-quoted symbol and the final argument is a literal
//! `(list …)`. `apply` spreads its last argument as the call's arguments, and a
//! literal `(list a b c)` *is* that argument list verbatim — so
//! `(apply #'f (list a b c))` is exactly `(f a b c)`. The `apply`, the `#'`, and
//! the `list` wrapper are all ceremony.
//!
//! Only the reducible shape is flagged, mirroring
//! [`crate::redundant_funcall::domain`]:
//!
//!   - `#'FN` reads as an atom carrying [`ReaderPrefix::Function`]; a direct
//!     `(FN …)` uses the identical lexical resolution. Equivalent.
//!   - The final argument must be a literal `(list …)`. `(apply #'f xs)` with a
//!     *variable* list argument is genuinely dynamic and never flagged.
//!   - There must be no arguments between the function and the list — the common
//!     `(apply #'f (list …))` form. `(apply #'f a b (list …))` (with fixed
//!     leading arguments) is left alone to keep the fix a single, obvious splice.
//!   - `(apply 'f …)` (ordinary quote) and `#'(lambda …)` are not matched, for
//!     the same reasons as `redundant-funcall`.
//!
//! The fix rewrites the whole form as `(FN ARGS…)`, copying the `list` form's
//! element source verbatim, so the rule is auto-fixable.
//!
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionKind, ExpressionView, Path as SexprPath, ReaderPrefix, SyntaxTree,
};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, is_paren_list, list_head};
use serde_json::{Value, json};

/// The sharp-quoted symbol name of `view` (`#'foo` → `foo`), or `None` when
/// `view` is not an atom carrying exactly the `#'` function prefix over a
/// non-empty symbol. Mirrors `redundant_funcall_report::sharp_quoted_symbol`.
fn sharp_quoted_symbol(view: &ExpressionView) -> Option<&str> {
    if view.kind != ExpressionKind::Atom {
        return None;
    }
    if view.reader_prefixes.as_slice() != [ReaderPrefix::Function] {
        return None;
    }
    let symbol = atom_text(view)?.get(view.symbol_offset..)?;
    (!symbol.is_empty()).then_some(symbol)
}

#[derive(Debug, Clone)]
pub struct RedundantApplyItem {
    /// The span of the whole `(apply #'FN (list …))` form.
    pub span: ByteSpan,
    /// The callee symbol name (`foo` for `#'foo`).
    pub callee: String,
    /// The span of the `list` form's arguments (`a b c` in `(list a b c)`), or
    /// `None` when the list is empty (`(list)` → `(foo)`).
    ///
    pub args_span: Option<ByteSpan>,
}

impl Finding for RedundantApplyItem {
    /// The rule's own name. The callee is a symbol read out of the source, not
    /// a tag from a closed set, so it stays a JSON field.
    fn kind(&self) -> &'static str {
        "redundant-apply"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("callee={}", self.callee)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("callee", json!(self.callee))]
    }

    fn message(&self) -> String {
        format!(
            "apply of #'{} to a literal list is a direct call; use ({} …)",
            self.callee, self.callee
        )
    }
}

pub fn examine_apply(
    view: &ExpressionView,
    apply_form_count: &mut usize,
    violations: &mut Vec<RedundantApplyItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("apply") {
        return;
    }
    *apply_form_count += 1;

    // children: [apply, #'fn, (list …)] — exactly the no-intermediate-args form.
    if view.children.len() != 3 {
        return;
    }
    let Some(callee) = sharp_quoted_symbol(&view.children[1]) else {
        return;
    };
    let list_form = &view.children[2];
    if !is_paren_list(list_form) {
        return;
    }
    if list_head(list_form).is_none_or(|head| !head.eq_ignore_ascii_case("list")) {
        return;
    }
    // The list form must carry no reader prefix (a plain `(list …)`).
    if !list_form.reader_prefixes.is_empty() {
        return;
    }

    // Arguments to splice: everything after `list`'s head, or none for `(list)`.
    let list_args = &list_form.children[1..];
    let args_span = match (list_args.first(), list_args.last()) {
        (Some(first), Some(last)) => Some(ByteSpan::new(first.span.start(), last.span.end())),
        _ => None,
    };

    violations.push(RedundantApplyItem {
        span: view.span,
        callee: callee.to_owned(),
        args_span,
    });
}

/// Collects every redundant `(apply #'fn (list …))` in one file, with the
/// number of `apply` forms scanned as the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_redundant_apply_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<RedundantApplyItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("apply_form_count", json!(0))],
        ));
    }

    let mut apply_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_apply(subview, &mut apply_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("apply_form_count", json!(apply_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<RedundantApplyItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_redundant_apply_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build redundant apply report")
    }

    fn applies(input: &str) -> (u64, Vec<RedundantApplyItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "apply_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("apply_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_apply_of_list_literal() {
        let source = "(apply #'foo (list a b))";
        let (count, violations) = applies(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].callee, "foo");
        assert_eq!(slice(source, violations[0].args_span.unwrap()), "a b");
    }

    #[test]
    fn flags_empty_list_as_a_no_argument_call() {
        let (_, violations) = applies("(apply #'foo (list))");
        assert_eq!(violations.len(), 1);
        assert!(violations[0].args_span.is_none());
    }

    #[test]
    fn preserves_compound_argument_source() {
        let source = "(apply #'foo (list (g x) 3))";
        let (_, violations) = applies(source);
        assert_eq!(slice(source, violations[0].args_span.unwrap()), "(g x) 3");
    }

    #[test]
    fn does_not_flag_variable_list_argument() {
        let (count, violations) = applies("(apply #'foo args)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_intermediate_arguments() {
        // (apply #'foo a (list b)) has a fixed leading arg; left alone.
        let (_, violations) = applies("(apply #'foo a (list b))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_ordinary_quoted_function() {
        let (_, violations) = applies("(apply 'foo (list a))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_sharp_quoted_lambda() {
        let (_, violations) = applies("(apply #'(lambda (x) x) (list a))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_funcall() {
        let (count, violations) = applies("(funcall #'foo (list a))");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_apply_and_list() {
        let (_, violations) = applies("(APPLY #'foo (LIST a b))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].callee, "foo");
    }

    #[test]
    fn finds_a_nested_redundant_apply() {
        let (_, violations) = applies("(progn (apply #'h (list g)))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].callee, "h");
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(apply #'foo (list a))", Dialect::Clojure)
            .expect("parse");
        let report = build_redundant_apply_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build redundant apply report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("apply_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(apply #'foo args)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_callee() {
        let report = report("(defun run (a b)\n  (apply #'process (list a b)))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "redundant-apply");
        assert_eq!(finding.json_fields(), vec![("callee", json!("process"))]);
        assert_eq!(finding.text_columns(), vec!["callee=process".to_owned()]);
    }

    #[test]
    fn the_summary_counts_every_apply_scanned_not_only_the_flagged_ones() {
        let report = report("(apply #'foo (list a))\n(apply #'bar args)\n");
        assert_eq!(report.summary, vec![("apply_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
