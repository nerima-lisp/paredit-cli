//! Common Lisp redundant-`identity` detection: a call `(identity X)`, which
//! returns `X` unchanged. `identity` is defined to return its single argument
//! verbatim — no coercion, no multiple-value truncation — so `(identity X)` is
//! exactly `X` and the call is pure noise (common after a higher-order default
//! `#'identity` is inlined, or in mechanically generated code).
//!
//! Only the one-argument call shape is flagged. A function *reference*
//! `#'identity` (used as a `:key`/`:test` argument, say) is a list head with a
//! reader prefix, not a call, and is left alone; an argument-mismatched
//! `(identity)` or `(identity a b)` is left to the arity rules.
//!
//! The fix replaces the whole form with the argument's exact source, so the rule
//! is auto-fixable.
//!
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{for_each_subview, list_head};
use serde_json::{Value, json};

#[derive(Debug, Clone)]
pub struct RedundantIdentityItem {
    /// The span of the whole `(identity X)` form.
    pub span: ByteSpan,
    /// The span of the argument `X` (lets a fix substitute its source).
    ///
    pub inner_span: ByteSpan,
}

impl Finding for RedundantIdentityItem {
    /// The rule's own name. Every finding is the same unwrapping; there is
    /// nothing to discriminate on.
    fn kind(&self) -> &'static str {
        "redundant-identity"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    /// None. The old text row carried the path and offset the envelope now
    /// prints itself, and nothing else.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    /// None. The old JSON carried only the path and span, both of which the
    /// envelope now emits itself.
    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        Vec::new()
    }

    fn message(&self) -> String {
        "identity returns its argument unchanged; (identity x) is x".to_owned()
    }
}

pub fn examine_identity(
    view: &ExpressionView,
    identity_form_count: &mut usize,
    violations: &mut Vec<RedundantIdentityItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("identity") {
        return;
    }
    *identity_form_count += 1;

    // children[0] is `identity`; a single argument means exactly two children.
    if view.children.len() != 2 {
        return;
    }

    violations.push(RedundantIdentityItem {
        span: view.span,
        inner_span: view.children[1].span,
    });
}

/// Collects every redundant `(identity X)` call in one file, with the number of
/// `identity` forms scanned as the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_redundant_identity_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<RedundantIdentityItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("identity_form_count", json!(0))],
        ));
    }

    let mut identity_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_identity(subview, &mut identity_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("identity_form_count", json!(identity_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<RedundantIdentityItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_redundant_identity_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build redundant identity report")
    }

    fn identities(input: &str) -> (u64, Vec<RedundantIdentityItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "identity_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("identity_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_identity_of_a_symbol() {
        let source = "(identity x)";
        let (count, violations) = identities(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].inner_span), "x");
    }

    #[test]
    fn preserves_compound_argument_source() {
        let source = "(identity (compute a b))";
        let (_, violations) = identities(source);
        assert_eq!(slice(source, violations[0].inner_span), "(compute a b)");
    }

    #[test]
    fn does_not_flag_zero_or_multi_argument() {
        assert!(identities("(identity)").1.is_empty());
        assert!(identities("(identity a b)").1.is_empty());
    }

    #[test]
    fn does_not_flag_a_function_reference() {
        // #'identity is a reference, not a call; sort's :key here is fine.
        let (count, violations) = identities("(sort xs #'< :key #'identity)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_the_head() {
        let (_, violations) = identities("(IDENTITY x)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_identity() {
        let (_, violations) = identities("(list (identity y))");
        assert_eq!(violations.len(), 1);
        assert_eq!(slice("(list (identity y))", violations[0].inner_span), "y");
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(identity x)", Dialect::Clojure).expect("parse");
        let report = build_redundant_identity_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build redundant identity report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("identity_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(identity)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_no_extra_fields() {
        let report = report("(defun echo (x)\n  (identity x))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "redundant-identity");
        assert!(finding.text_columns().is_empty());
        assert!(finding.json_fields().is_empty());
    }

    #[test]
    fn the_summary_counts_every_identity_scanned_not_only_the_flagged_ones() {
        let report = report("(identity x)\n(identity a b)\n");
        assert_eq!(report.summary, vec![("identity_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
