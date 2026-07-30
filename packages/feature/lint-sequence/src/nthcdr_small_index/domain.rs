//! Common Lisp `nthcdr`-small-index detection: an `(nthcdr n list)` whose count
//! operand is a literal `1`, `2`, `3`, or `4`, for which the language provides a
//! named `cdr` accessor — `(nthcdr 1 x)` is `(cdr x)`, `(nthcdr 2 x)` is
//! `(cddr x)`, `(nthcdr 3 x)` is `(cdddr x)`, and `(nthcdr 4 x)` is
//! `(cddddr x)`. The standard *defines* `cddr`…`cddddr` as exactly these nested
//! `cdr` calls, so the rewrite is exact (same tail cons, `x` read once,
//! identical nil-on-overrun) and the accessor name reads more directly than a
//! bare count.
//!
//! Only the bare decimal literals `1`–`4` are matched. The count `0` is
//! [`crate::nthcdr_zero::domain`]'s concern (it is the identity, not a
//! `cdr` chain), and there is no `cdddddr`, so `5` and up are left alone. A
//! float `1.0`, a `#x1`/prefixed spelling, a variable count, and a
//! reader-conditional operand are all left alone.
//!
//! The fix rewrites `(nthcdr n list)` as `(ACCESSOR list)`, copying the list
//! operand's source verbatim, so the rule is auto-fixable.
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

/// The named `cdr` accessor for a literal count `1`–`4`, or `None` for any other
/// spelling (`0`, `5`+, floats, prefixed, or non-numeric).
fn small_index_accessor(view: &ExpressionView) -> Option<&'static str> {
    if !view.reader_prefixes.is_empty() {
        return None;
    }
    match atom_text(view)? {
        "1" => Some("cdr"),
        "2" => Some("cddr"),
        "3" => Some("cdddr"),
        "4" => Some("cddddr"),
        _ => None,
    }
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct NthcdrSmallIndexItem {
    /// The span of the whole `(nthcdr n list)` form.
    pub span: ByteSpan,
    /// The named accessor to rewrite to (`cdr`, `cddr`, `cdddr`, `cddddr`).
    pub accessor: &'static str,
    /// The span of the list operand (for reconstructing the fix).
    ///
    /// The rewrite's input, but the old report published it and a consumer
    /// synthesizing its own `(cddr …)` needs it, so it stays on the report.
    pub list_span: ByteSpan,
}

impl Finding for NthcdrSmallIndexItem {
    /// The `cdr` accessor the count maps to, so `(nthcdr 1 …)` and
    /// `(nthcdr 4 …)` are separable without parsing JSON.
    ///
    /// A closed set of four `&'static str` values this module already models,
    /// and the one a consumer would filter on — a lone `cdr` and a `cddddr`
    /// are very different things to find in a codebase.
    fn kind(&self) -> &'static str {
        self.accessor
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    /// Nothing: the old text row's only column beyond the path and offset was
    /// the bare accessor, which now leads the row as the `kind`.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("accessor", json!(self.accessor)),
            ("list_span", span_json(self.list_span)),
        ]
    }

    /// The same sentence the `nthcdr-small-index` lint rule writes, so a SARIF
    /// or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!(
            "nthcdr with a small count has a named cdr accessor; use ({} …)",
            self.accessor
        )
    }
}

/// A sub-span in the shape the old hand-written report published it.
fn span_json(span: ByteSpan) -> Value {
    json!({ "start": span.start().get(), "end": span.end().get() })
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    nthcdr_form_count: &mut usize,
    violations: &mut Vec<NthcdrSmallIndexItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("nthcdr") {
        return;
    }
    *nthcdr_form_count += 1;

    // children: [nthcdr, count, list] — require exactly the two-operand shape.
    if view.children.len() != 3 {
        return;
    }
    let count = &view.children[1];
    let list = &view.children[2];
    if is_reader_conditional(count) || is_reader_conditional(list) {
        return;
    }
    let Some(accessor) = small_index_accessor(count) else {
        return;
    };

    violations.push(NthcdrSmallIndexItem {
        span: view.span,
        accessor,
        list_span: list.span,
    });
}

/// Collects every `(nthcdr 1..=4 list)` in one file, with the number of
/// `nthcdr` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no small-count nthcdr here" for Common
/// Lisp and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_nthcdr_small_index_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<NthcdrSmallIndexItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("nthcdr_form_count", json!(0))],
        ));
    }

    let mut nthcdr_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, &mut nthcdr_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("nthcdr_form_count", json!(nthcdr_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<NthcdrSmallIndexItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_nthcdr_small_index_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build nthcdr small index report")
    }

    /// The `(nthcdr_form_count, violations)` pair the report is built from.
    fn nthcdrs(input: &str) -> (u64, Vec<NthcdrSmallIndexItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "nthcdr_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("nthcdr_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_one_as_cdr() {
        let source = "(nthcdr 1 items)";
        let (count, violations) = nthcdrs(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].accessor, "cdr");
        assert_eq!(slice(source, violations[0].list_span), "items");
    }

    #[test]
    fn maps_each_small_index_to_its_accessor() {
        assert_eq!(nthcdrs("(nthcdr 2 x)").1[0].accessor, "cddr");
        assert_eq!(nthcdrs("(nthcdr 3 x)").1[0].accessor, "cdddr");
        assert_eq!(nthcdrs("(nthcdr 4 x)").1[0].accessor, "cddddr");
    }

    #[test]
    fn preserves_a_compound_list() {
        let source = "(nthcdr 2 (rest xs))";
        let (_, violations) = nthcdrs(source);
        assert_eq!(slice(source, violations[0].list_span), "(rest xs)");
    }

    #[test]
    fn does_not_flag_zero() {
        // (nthcdr 0 x) is the identity, nthcdr-zero's concern.
        let (count, violations) = nthcdrs("(nthcdr 0 x)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_five_or_more() {
        // There is no cdddddr, so 5 has no named accessor.
        let (_, violations) = nthcdrs("(nthcdr 5 x)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_float_or_variable() {
        assert!(nthcdrs("(nthcdr 1.0 x)").1.is_empty());
        assert!(nthcdrs("(nthcdr n x)").1.is_empty());
    }

    #[test]
    fn does_not_flag_missing_list_operand() {
        let (count, violations) = nthcdrs("(nthcdr 2)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn flags_uppercase_head() {
        let (_, violations) = nthcdrs("(NTHCDR 3 x)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].accessor, "cdddr");
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = nthcdrs("(defun f (x) (nthcdr 1 x))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(nthcdr 1 x)", Dialect::Clojure).expect("parse");
        let report = build_nthcdr_small_index_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build nthcdr small index report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("nthcdr_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(nthcdr n x)").dialect_modelled);
    }

    #[test]
    fn a_finding_leads_with_its_accessor_and_keeps_the_list_span() {
        let source = "(defun f (xs)\n  (nthcdr 2 (rest xs)))\n";
        let report = report(source);
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        // The accessor is the kind, so the text row leads with it and carries
        // no further columns.
        assert_eq!(finding.kind(), "cddr");
        assert_eq!(slice(source, finding.list_span), "(rest xs)");
        assert_eq!(
            finding.json_fields(),
            vec![
                ("accessor", json!("cddr")),
                ("list_span", span_json(finding.list_span)),
            ]
        );
        assert!(finding.text_columns().is_empty());
    }

    #[test]
    fn the_summary_counts_every_nthcdr_form_not_only_the_flagged_ones() {
        let report = report("(nthcdr 1 x)\n(nthcdr 0 x)\n(nthcdr 5 x)\n");
        assert_eq!(report.summary, vec![("nthcdr_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }
}
