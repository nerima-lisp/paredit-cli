//! Common Lisp duplicate parallel-`let`-binding detection: a `let` form whose
//! binding list names the same variable more than once. The standard forbids
//! this — "the consequences are undefined if more than one binding of the
//! same name appears" in a parallel `let` — because all a `let`'s
//! initializers run in the outer scope simultaneously, so a repeated name is
//! not shadowing but an outright error.
//!
//! Scoped to `let` on purpose: `let*` binds sequentially, where re-binding a
//! name is legal, intentional shadowing (each later init sees the earlier
//! binding), so it is excluded here. This report walks the whole tree via the
//! shared [`paredit_core_syntax::view_query::for_each_subview`], since a `let` can
//! appear anywhere in a body.
//!
//! Scope: Common Lisp only. A binding is read as either a bare symbol
//! (`x`, initialized to nil) or a `(name init)` list; names fold ASCII case
//! the way the reader does.

use std::collections::BTreeMap;
use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding, line_of};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, is_paren_list, list_head};
use serde_json::{Value, json};

/// The bound variable name of one `let` binding: a bare symbol, or the head
/// of a `(name init)` list.
fn binding_name(binding: &ExpressionView) -> Option<&str> {
    atom_text(binding).or_else(|| {
        is_paren_list(binding)
            .then(|| binding.children.first().and_then(atom_text))
            .flatten()
    })
}

#[derive(Debug, Clone)]
pub struct DuplicateLetBindingItem {
    pub span: ByteSpan,
    /// The 1-based line the `let` form starts on.
    pub line: usize,
    pub name: String,
    pub occurrence_count: usize,
}

impl Finding for DuplicateLetBindingItem {
    /// The rule's own name, not the variable.
    ///
    /// The variable is read from the source and so is an open set, while `kind`
    /// is `&'static str`. It stays a text column and a JSON field instead.
    fn kind(&self) -> &'static str {
        "duplicate-let-bindings"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("name={}", self.name),
            format!("count={}", self.occurrence_count),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("name", json!(self.name)),
            ("occurrence_count", json!(self.occurrence_count)),
        ]
    }

    /// The same sentence the `duplicate-let-bindings` lint rule writes, so a
    /// SARIF or JUnit consumer reading both sees one defect described one way.
    fn message(&self) -> String {
        format!(
            "let binds {} more than once ({}×)",
            self.name, self.occurrence_count
        )
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_let(
    view: &ExpressionView,
    source: &str,
    let_form_count: &mut usize,
    duplicates: &mut Vec<DuplicateLetBindingItem>,
) {
    // Exactly `let` — `let*` binds sequentially, where re-binding is legal.
    if !list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("let")) {
        return;
    }
    let Some(binding_list) = view.children.get(1) else {
        return;
    };
    if !is_paren_list(binding_list) {
        return;
    }
    *let_form_count += 1;

    // Preserve first-seen spelling and binding order while counting.
    let mut order: Vec<String> = Vec::new();
    let mut counts: BTreeMap<String, (String, usize)> = BTreeMap::new();
    for name in binding_list.children.iter().filter_map(binding_name) {
        let needle = name.to_ascii_uppercase();
        let entry = counts.entry(needle.clone()).or_insert_with(|| {
            order.push(needle);
            (name.to_owned(), 0)
        });
        entry.1 += 1;
    }

    for needle in order {
        let (name, occurrence_count) = &counts[&needle];
        if *occurrence_count < 2 {
            continue;
        }
        duplicates.push(DuplicateLetBindingItem {
            span: view.span,
            line: line_of(source, view.span.start().get()),
            name: name.clone(),
            occurrence_count: *occurrence_count,
        });
    }
}

/// Collects every duplicated parallel-`let` binding in one file, with the
/// number of `let` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no repeated binding here" for Common
/// Lisp and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_duplicate_let_binding_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<DuplicateLetBindingItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("let_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut let_form_count = 0;
    let mut duplicates = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_let(subview, source, &mut let_form_count, &mut duplicates);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        duplicates,
        vec![("let_form_count", json!(let_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<DuplicateLetBindingItem> {
        let tree = SyntaxTree::parse(input).expect("parse input");
        build_duplicate_let_binding_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build duplicate let binding report")
    }

    /// The `(let_form_count, duplicates)` pair the report is built from.
    fn duplicates(input: &str) -> (u64, Vec<DuplicateLetBindingItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "let_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("let_form_count in the summary");
        (count, report.findings)
    }

    #[test]
    fn flags_a_variable_bound_twice_in_a_parallel_let() {
        let (let_form_count, duplicates) = duplicates("(let ((x 1) (y 2) (x 3)) x)");
        assert_eq!(let_form_count, 1);
        assert_eq!(duplicates.len(), 1);
        assert_eq!(duplicates[0].name, "x");
        assert_eq!(duplicates[0].occurrence_count, 2);
    }

    #[test]
    fn flags_a_bare_symbol_binding_duplicated() {
        let (_, duplicates) = duplicates("(let (x x) x)");
        assert_eq!(duplicates.len(), 1);
        assert_eq!(duplicates[0].name, "x");
    }

    #[test]
    fn folds_symbol_case() {
        let (_, duplicates) = duplicates("(let ((x 1) (X 2)) x)");
        assert_eq!(duplicates.len(), 1);
    }

    #[test]
    fn does_not_flag_distinct_bindings() {
        let (let_form_count, duplicates) = duplicates("(let ((x 1) (y 2)) x)");
        assert_eq!(let_form_count, 1);
        assert!(duplicates.is_empty());
    }

    #[test]
    fn does_not_flag_a_let_star_which_allows_shadowing() {
        let (let_form_count, duplicates) = duplicates("(let* ((x 1) (x (1+ x))) x)");
        assert_eq!(let_form_count, 0);
        assert!(duplicates.is_empty());
    }

    #[test]
    fn finds_a_let_nested_in_a_function_body() {
        let (let_form_count, duplicates) = duplicates("(defun f () (let ((a 1) (a 2)) a))");
        assert_eq!(let_form_count, 1);
        assert_eq!(duplicates.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse("(let ((x 1) (x 2)) x)").expect("parse input");
        let report =
            build_duplicate_let_binding_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build duplicate let binding report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("let_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(let ((x 1)) x)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_columns() {
        let report = report("(defun f ()\n  (let ((a 1) (a 2)) a))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "duplicate-let-bindings");
        assert_eq!(
            finding.text_columns(),
            vec!["name=a".to_owned(), "count=2".to_owned()]
        );
        assert_eq!(
            finding.json_fields(),
            vec![("name", json!("a")), ("occurrence_count", json!(2))]
        );
    }

    #[test]
    fn the_summary_counts_every_let_scanned_not_only_the_flagged_ones() {
        let report = report("(let ((x 1) (x 2)) x)\n(let ((y 1)) y)\n");
        assert_eq!(report.summary, vec![("let_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
