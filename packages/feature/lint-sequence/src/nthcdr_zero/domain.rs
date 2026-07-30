//! Common Lisp `nthcdr`-zero detection: an `(nthcdr 0 list)` whose count
//! operand is the literal `0`. Per CLHS, `nthcdr` with `n` equal to `0` returns
//! the list itself, so `(nthcdr 0 list)` is exactly `list` — same value, no
//! traversal. Dropping the wrapper leaves the plain list the call already
//! yields.
//!
//! Only the bare integer literal `0` is matched. A float `0.0` is left alone:
//! it is not an integer index, so `(nthcdr 0.0 x)` is not the recognized shape.
//! A non-`0` count, a `#x0`/prefixed spelling, a variable count, and a
//! reader-conditional operand are all left alone.
//!
//! The fix rewrites `(nthcdr 0 list)` as `list`, copying the list operand from
//! its exact source, so the rule is auto-fixable.
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

/// Whether `view` is the bare integer `0` literal (no reader prefixes, so `#x0`
/// and a prefixed `,0` are excluded; `0.0` is a different spelling, excluded).
fn is_zero_literal(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty() && atom_text(view) == Some("0")
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct NthcdrZeroItem {
    /// The span of the whole `(nthcdr 0 list)` form.
    pub span: ByteSpan,
    /// The span of the list operand.
    ///
    /// The rewrite's input, not the report's: the lint rule copies the list
    /// source from it to replace the whole call, and the command has never
    /// printed it.
    pub list_span: ByteSpan,
}

impl Finding for NthcdrZeroItem {
    /// The rule's own name. There is one shape here — a literal `0` count —
    /// with nothing to separate findings by, so the kind names the rule.
    fn kind(&self) -> &'static str {
        "nthcdr-zero"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    /// None: the old text row carried nothing past the path and the offset,
    /// and the leading `kind` already says which rule spoke.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    /// None: the old JSON carried only `path` and `span`, both of which the
    /// envelope now supplies.
    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        Vec::new()
    }

    /// The same sentence the `nthcdr-zero` lint rule writes, so a SARIF or
    /// JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        "nthcdr with a zero count returns the list unchanged; (nthcdr 0 x) is x".to_owned()
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    nthcdr_form_count: &mut usize,
    violations: &mut Vec<NthcdrZeroItem>,
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
    if !is_zero_literal(count) {
        return;
    }

    violations.push(NthcdrZeroItem {
        span: view.span,
        list_span: list.span,
    });
}

/// Collects every `(nthcdr 0 list)` in one file, with the number of `nthcdr`
/// forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no zero-count nthcdr here" for Common
/// Lisp and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_nthcdr_zero_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<NthcdrZeroItem>> {
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

    fn report(input: &str) -> FileFindings<NthcdrZeroItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_nthcdr_zero_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build nthcdr zero report")
    }

    /// The `(nthcdr_form_count, violations)` pair the report is built from.
    fn nthcdrs(input: &str) -> (u64, Vec<NthcdrZeroItem>) {
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
    fn flags_nthcdr_zero() {
        let source = "(nthcdr 0 items)";
        let (count, violations) = nthcdrs(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].list_span), "items");
    }

    #[test]
    fn preserves_a_compound_list() {
        let source = "(nthcdr 0 (rest xs))";
        let (_, violations) = nthcdrs(source);
        assert_eq!(slice(source, violations[0].list_span), "(rest xs)");
    }

    #[test]
    fn does_not_flag_nonzero_index() {
        let (count, violations) = nthcdrs("(nthcdr 1 x)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_float_zero() {
        // (nthcdr 0.0 x) is not an integer index; not the recognized shape.
        let (_, violations) = nthcdrs("(nthcdr 0.0 x)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_variable_index() {
        let (_, violations) = nthcdrs("(nthcdr n x)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_two_children() {
        // (nthcdr 0) is missing the list operand; not the recognized shape.
        let (count, violations) = nthcdrs("(nthcdr 0)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn flags_uppercase_head() {
        let (_, violations) = nthcdrs("(NTHCDR 0 x)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = nthcdrs("(defun f (x) (nthcdr 0 x))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(nthcdr 0 x)", Dialect::Clojure).expect("parse");
        let report = build_nthcdr_zero_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build nthcdr zero report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("nthcdr_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(nthcdr n x)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_no_extra_fields() {
        let report = report("(defun f (x)\n  (nthcdr 0 x))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "nthcdr-zero");
        assert!(finding.json_fields().is_empty());
        assert!(finding.text_columns().is_empty());
    }

    #[test]
    fn the_summary_counts_every_nthcdr_scanned_not_only_the_flagged_ones() {
        let report = report("(nthcdr 0 a)\n(nthcdr 1 b)\n(nthcdr 0 c)\n");
        assert_eq!(report.summary, vec![("nthcdr_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 2);
    }
}
