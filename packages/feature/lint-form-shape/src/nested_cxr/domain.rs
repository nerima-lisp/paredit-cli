//! Common Lisp nested-`cXr` detection: a `car`/`cdr`-family accessor applied
//! directly to another `car`/`cdr`-family accessor, which the standard combines
//! into a single accessor — `(car (cdr x))` is `(cadr x)`, `(cdr (cdr x))` is
//! `(cddr x)`, `(car (cddr x))` is `(caddr x)`. The composite `cXr` names are
//! *defined* as exactly these nestings, so the collapse is an exact rewrite that
//! reads the argument once, and the combined form is the idiomatic spelling.
//!
//! A `cXr` accessor is `c` followed by one to four `a`/`d` letters and a final
//! `r` (`car`, `cdr`, `caar`, …, `cddddr`). A nesting is flagged only when the
//! outer accessor has exactly one argument (the inner accessor form), the inner
//! accessor has exactly one argument, and the two middles concatenated are still
//! at most four letters — i.e. the combined accessor is itself a standard `cXr`.
//! Deeper nestings collapse one layer per pass under `--fix`'s fixpoint, so
//! `(car (cdr (cdr x)))` converges to `(caddr x)`.
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

/// The `a`/`d` middle of a `cXr` accessor symbol (`cadr` → `ad`), or `None` when
/// `symbol` is not a one-to-four-letter `c…r` accessor. Case-insensitive.
fn cxr_middle(symbol: &str) -> Option<String> {
    let lower = symbol.to_ascii_lowercase();
    let middle = lower.strip_prefix('c')?.strip_suffix('r')?;
    if middle.is_empty() || middle.len() > 4 {
        return None;
    }
    middle
        .bytes()
        .all(|byte| byte == b'a' || byte == b'd')
        .then(|| middle.to_owned())
}

#[derive(Debug, Clone)]
pub struct NestedCxrItem {
    /// The span of the whole `(OUTER (INNER x))` form.
    pub span: ByteSpan,
    /// The combined accessor name (`cadr` for `(car (cdr x))`).
    pub combined: String,
    /// The span of the innermost argument `x` (for reconstructing the fix).
    ///
    pub arg_span: ByteSpan,
}

impl Finding for NestedCxrItem {
    /// The rule's own name. `combined` is one of thirty accessor spellings
    /// built per finding, not a tag from a closed set, so it stays a JSON
    /// field rather than becoming the kind.
    fn kind(&self) -> &'static str {
        "nested-cxr"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("combined={}", self.combined)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("combined", json!(self.combined))]
    }

    fn message(&self) -> String {
        format!(
            "nested car/cdr accessors combine into ({} …)",
            self.combined
        )
    }
}

pub fn examine_accessor(
    view: &ExpressionView,
    accessor_form_count: &mut usize,
    violations: &mut Vec<NestedCxrItem>,
) {
    let Some(outer_head) = list_head(view) else {
        return;
    };
    let Some(outer_middle) = cxr_middle(outer_head) else {
        return;
    };
    *accessor_form_count += 1;

    // The outer accessor must take exactly one argument (the inner form).
    if view.children.len() != 2 {
        return;
    }
    let inner = &view.children[1];
    let Some(inner_head) = list_head(inner) else {
        return;
    };
    let Some(inner_middle) = cxr_middle(inner_head) else {
        return;
    };
    // The inner accessor must take exactly one argument.
    if inner.children.len() != 2 {
        return;
    }

    // The combined accessor must itself be a standard cXr (<= 4 middle letters).
    if outer_middle.len() + inner_middle.len() > 4 {
        return;
    }
    let combined = format!("c{outer_middle}{inner_middle}r");

    violations.push(NestedCxrItem {
        span: view.span,
        combined,
        arg_span: inner.children[1].span,
    });
}

/// Collects every combinable nested `cXr` accessor in one file, with the number
/// of `cXr` accessor forms scanned as the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_nested_cxr_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<NestedCxrItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("accessor_form_count", json!(0))],
        ));
    }

    let mut accessor_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_accessor(subview, &mut accessor_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("accessor_form_count", json!(accessor_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<NestedCxrItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_nested_cxr_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build nested cxr report")
    }

    fn cxrs(input: &str) -> (u64, Vec<NestedCxrItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "accessor_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("accessor_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn combines_car_of_cdr_into_cadr() {
        let source = "(car (cdr x))";
        let (count, violations) = cxrs(source);
        // Both the outer `car` and the inner `cdr` are cXr forms scanned.
        assert_eq!(count, 2);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].combined, "cadr");
        assert_eq!(slice(source, violations[0].arg_span), "x");
    }

    #[test]
    fn combines_cdr_of_cdr_into_cddr() {
        let (_, violations) = cxrs("(cdr (cdr items))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].combined, "cddr");
    }

    #[test]
    fn combines_multi_letter_accessors() {
        // (car (cddr x)) -> caddr
        let (_, violations) = cxrs("(car (cddr x))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].combined, "caddr");
    }

    #[test]
    fn preserves_a_compound_argument() {
        let source = "(car (cdr (lookup k)))";
        let (_, violations) = cxrs(source);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].combined, "cadr");
        assert_eq!(slice(source, violations[0].arg_span), "(lookup k)");
    }

    #[test]
    fn does_not_flag_when_combined_exceeds_four_letters() {
        // caddr of caddr would be caddaddr (6) — not a standard accessor.
        let (_, violations) = cxrs("(caddr (caddr x))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_single_accessor() {
        let (count, violations) = cxrs("(car x)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_car_of_a_non_accessor() {
        let (_, violations) = cxrs("(car (reverse x))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_first_or_rest() {
        // first/rest are not cXr spellings.
        let (count, violations) = cxrs("(first (rest x))");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_multi_argument_accessor() {
        // A malformed 2-arg car is left to accessor/arity rules.
        let (_, violations) = cxrs("(car (cdr x) y)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_the_accessors() {
        let (_, violations) = cxrs("(CAR (CDR x))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].combined, "cadr");
    }

    #[test]
    fn finds_a_nested_accessor_inside_a_form() {
        let (_, violations) = cxrs("(list (car (cdr pair)))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].combined, "cadr");
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(car (cdr x))", Dialect::Clojure).expect("parse");
        let report = build_nested_cxr_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build nested cxr report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("accessor_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(car x)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_combined_accessor() {
        let report = report("(defun second-of (pair)\n  (car (cdr pair)))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "nested-cxr");
        assert_eq!(finding.json_fields(), vec![("combined", json!("cadr"))]);
        assert_eq!(finding.text_columns(), vec!["combined=cadr".to_owned()]);
    }

    #[test]
    fn the_summary_counts_every_accessor_scanned_not_only_the_flagged_ones() {
        let report = report("(car (cdr x))\n(car y)\n");
        assert_eq!(report.summary, vec![("accessor_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }
}
