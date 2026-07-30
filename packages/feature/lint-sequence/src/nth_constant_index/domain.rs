//! Common Lisp nth-constant-index detection: an `nth` with a small literal
//! index, which the language provides a named ordinal accessor for — `(nth 0 x)`
//! is `(first x)`, `(nth 1 x)` is `(second x)`, up to `(nth 9 x)` is
//! `(tenth x)`. The standard *defines* `first`…`tenth` as exactly these `nth`
//! calls, so the rewrite is exact (same element, `x` read once, identical
//! nil-on-overrun) and the ordinal name reads more clearly than a bare index.
//!
//! Only a bare decimal literal `0`–`9` is flagged (there is no `eleventh`). A
//! variable or computed index (`(nth i x)`) is genuinely dynamic and left alone,
//! as is any other radix or prefixed spelling.
//!
//! The fix rewrites `(nth N x)` as `(ORDINAL x)`, copying the list argument's
//! source verbatim, so the rule is auto-fixable.
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

/// The ordinal accessor for a bare decimal index `0`–`9`, or `None` otherwise.
fn ordinal_for_index(view: &ExpressionView) -> Option<&'static str> {
    if !view.reader_prefixes.is_empty() {
        return None;
    }
    match atom_text(view)? {
        "0" => Some("first"),
        "1" => Some("second"),
        "2" => Some("third"),
        "3" => Some("fourth"),
        "4" => Some("fifth"),
        "5" => Some("sixth"),
        "6" => Some("seventh"),
        "7" => Some("eighth"),
        "8" => Some("ninth"),
        "9" => Some("tenth"),
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub struct NthConstantIndexItem {
    /// The span of the whole `(nth N x)` form.
    pub span: ByteSpan,
    /// The ordinal accessor name (`first` for `(nth 0 x)`).
    pub ordinal: &'static str,
    /// The span of the list argument `x` (for reconstructing the fix).
    ///
    /// The rewrite's input only: the old report never published it, and the
    /// finding's own span already locates the call for a consumer.
    pub list_span: ByteSpan,
}

impl Finding for NthConstantIndexItem {
    /// The ordinal the index maps to, so `(nth 0 …)` and `(nth 9 …)` are
    /// separable without parsing JSON.
    ///
    /// A closed set of ten `&'static str` values this module already models,
    /// and the one a consumer would filter on — "show me every `first`" is a
    /// real question about a codebase's shape.
    fn kind(&self) -> &'static str {
        self.ordinal
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    /// Nothing: the old text row's only column beyond the path and offset was
    /// the ordinal, which now leads the row as the `kind`.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("ordinal", json!(self.ordinal))]
    }

    /// The same sentence the `nth-constant-index` lint rule writes, so a SARIF
    /// or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!("nth with a constant index; use ({} …)", self.ordinal)
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_nth(
    view: &ExpressionView,
    nth_form_count: &mut usize,
    violations: &mut Vec<NthConstantIndexItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("nth") {
        return;
    }
    *nth_form_count += 1;

    // children: [nth, index, list] — exactly the two-argument shape.
    if view.children.len() != 3 {
        return;
    }
    let Some(ordinal) = ordinal_for_index(&view.children[1]) else {
        return;
    };

    violations.push(NthConstantIndexItem {
        span: view.span,
        ordinal,
        list_span: view.children[2].span,
    });
}

/// Collects every constant-index `nth` in one file, with the number of `nth`
/// forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no constant index here" for Common Lisp
/// and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_nth_constant_index_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<NthConstantIndexItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("nth_form_count", json!(0))],
        ));
    }

    let mut nth_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_nth(subview, &mut nth_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("nth_form_count", json!(nth_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<NthConstantIndexItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_nth_constant_index_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build nth constant index report")
    }

    /// The `(nth_form_count, violations)` pair the report is built from.
    fn nths(input: &str) -> (u64, Vec<NthConstantIndexItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "nth_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("nth_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn zero_is_first() {
        let source = "(nth 0 xs)";
        let (count, violations) = nths(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].ordinal, "first");
        assert_eq!(slice(source, violations[0].list_span), "xs");
    }

    #[test]
    fn nine_is_tenth() {
        let (_, violations) = nths("(nth 9 items)");
        assert_eq!(violations[0].ordinal, "tenth");
    }

    #[test]
    fn maps_each_small_index() {
        let expected = [
            ("(nth 1 x)", "second"),
            ("(nth 2 x)", "third"),
            ("(nth 3 x)", "fourth"),
            ("(nth 4 x)", "fifth"),
            ("(nth 5 x)", "sixth"),
            ("(nth 6 x)", "seventh"),
            ("(nth 7 x)", "eighth"),
            ("(nth 8 x)", "ninth"),
        ];
        for (src, ordinal) in expected {
            let (_, violations) = nths(src);
            assert_eq!(violations[0].ordinal, ordinal, "for {src}");
        }
    }

    #[test]
    fn preserves_compound_list_source() {
        let source = "(nth 0 (rest pairs))";
        let (_, violations) = nths(source);
        assert_eq!(slice(source, violations[0].list_span), "(rest pairs)");
    }

    #[test]
    fn does_not_flag_index_ten_or_higher() {
        let (count, violations) = nths("(nth 10 x)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_variable_index() {
        let (_, violations) = nths("(nth i x)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_other_heads() {
        let (count, violations) = nths("(elt x 0)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_the_head() {
        let (_, violations) = nths("(NTH 0 xs)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].ordinal, "first");
    }

    #[test]
    fn finds_a_nested_nth() {
        let (_, violations) = nths("(list (nth 2 row))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].ordinal, "third");
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(nth 0 xs)", Dialect::Clojure).expect("parse");
        let report = build_nth_constant_index_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build nth constant index report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("nth_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(nth i xs)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_leads_with_its_ordinal() {
        let report = report("(defun head-of (xs)\n  (nth 0 xs))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        // The ordinal is the kind, so the text row leads with it and carries
        // no further columns.
        assert_eq!(finding.kind(), "first");
        assert_eq!(finding.json_fields(), vec![("ordinal", json!("first"))]);
        assert!(finding.text_columns().is_empty());
    }

    #[test]
    fn the_summary_counts_every_nth_form_not_only_the_flagged_ones() {
        let report = report("(nth 0 xs)\n(nth 10 xs)\n(nth i xs)\n");
        assert_eq!(report.summary, vec![("nth_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }
}
