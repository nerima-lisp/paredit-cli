//! Common Lisp destructive-operation-on-a-literal detection: a destructive
//! sequence function whose sequence argument is a *quoted list literal* —
//! `(nreverse '(a b c))`, `(sort '(3 1 2) #'<)`, `(nbutlast '(1 2 3))`. These
//! functions are permitted to modify (and reuse the cells of) their argument,
//! but a quoted literal is a constant: the standard leaves the effect of
//! modifying it undefined, and implementations may coalesce identical literals
//! or place them in read-only memory. The result is a latent bug that "works"
//! until the literal is shared — the fix is to build a fresh list with
//! `(list …)` or `copy-list`.
//!
//! Destructive functions are covered with the argument position(s) where a
//! quoted literal would be modified — first-argument sequences (`nreverse`,
//! `nreconc`, `sort`, `stable-sort`, `nbutlast`, `delete-duplicates`,
//! `rplaca`, `rplacd`), later-argument sequences (`delete`/`delete-if`,
//! `nsublis`, `nsubstitute`/`nsubst` and their variants), the two set-operation
//! lists (`nunion`, `nintersection`, `nset-difference`, `nset-exclusive-or`),
//! and every argument but the last of `nconc`. Only a quoted, *non-empty* list
//! literal is flagged (`'()` is `nil`, harmless); both the reader form (`'(…)`)
//! and the explicit `(quote (…))` are recognized. A fresh list (`(list …)`), a
//! variable, or a `copy-list` result is left alone.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::expression_equality::render_expression;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionView, Path as SexprPath, ReaderPrefix, SyntaxTree,
};
use paredit_core_syntax::view_query::{for_each_subview, is_paren_list, list_head};
use serde_json::{Value, json};

/// The argument indices at which a destructive function may modify its
/// argument (so a quoted literal there is undefined behavior), or `None` if the
/// head is not a covered destructive function. Required positional arguments sit
/// at fixed indices — keyword arguments follow them — so these positions are
/// stable regardless of `:test`/`:key`/etc.
fn sequence_indices(head: &str, child_count: usize) -> Option<Vec<usize>> {
    let eq = |name: &str| head.eq_ignore_ascii_case(name);
    if eq("nreverse")
        || eq("nreconc")
        || eq("sort")
        || eq("stable-sort")
        || eq("nbutlast")
        || eq("delete-duplicates")
        || eq("rplaca")
        || eq("rplacd")
    {
        Some(vec![1])
    } else if eq("delete") || eq("delete-if") || eq("delete-if-not") || eq("nsublis") {
        Some(vec![2])
    } else if eq("nsubstitute")
        || eq("nsubstitute-if")
        || eq("nsubstitute-if-not")
        || eq("nsubst")
        || eq("nsubst-if")
        || eq("nsubst-if-not")
    {
        Some(vec![3])
    } else if eq("nunion")
        || eq("nintersection")
        || eq("nset-difference")
        || eq("nset-exclusive-or")
    {
        Some(vec![1, 2])
    } else if eq("nconc") {
        // nconc modifies every argument except the last.
        Some((1..child_count.saturating_sub(1)).collect())
    } else {
        None
    }
}

/// Whether `view` is a quoted, non-empty list — either `'(a b)` (a paren list
/// carrying a `Quote` reader prefix) or `(quote (a b))`. An empty quoted list
/// (`'()`) is `nil` and is not a modifiable list literal.
fn is_quoted_list_literal(view: &ExpressionView) -> bool {
    if is_paren_list(view)
        && view.reader_prefixes.contains(&ReaderPrefix::Quote)
        && !view.children.is_empty()
    {
        return true;
    }

    if list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("quote")) {
        if let Some(quoted) = view.children.get(1) {
            return is_paren_list(quoted) && !quoted.children.is_empty();
        }
    }

    false
}

#[derive(Debug, Clone)]
pub struct DestructiveLiteralItem {
    pub span: ByteSpan,
    /// The 1-based line the call starts on.
    pub line: usize,
    /// The destructive operator (`nreverse`, `sort`, …).
    pub operator: String,
    /// A rendered form of the quoted literal being modified.
    pub literal: String,
}

impl Finding for DestructiveLiteralItem {
    /// The rule's name, not the operator.
    ///
    /// There are 23 covered destructive functions and `operator` is a
    /// per-finding `String` (lowercased from source), so it cannot be a
    /// `&'static str` kind. It stays a JSON field and a text column, where a
    /// consumer can still filter on it.
    fn kind(&self) -> &'static str {
        "destructive-literal"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        vec![self.operator.clone(), format!("literal={}", self.literal)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("operator", json!(self.operator)),
            ("literal", json!(self.literal)),
        ]
    }

    /// The same sentence the `destructive-literal` lint rule writes, so a SARIF
    /// or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!(
            "{} destructively modifies quoted literal {} (undefined behavior)",
            self.operator, self.literal
        )
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_call(
    view: &ExpressionView,
    source: &str,
    destructive_call_count: &mut usize,
    violations: &mut Vec<DestructiveLiteralItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let Some(indices) = sequence_indices(head, view.children.len()) else {
        return;
    };
    *destructive_call_count += 1;

    // Report the first argument slot that holds a quoted literal; one call with
    // two literal arguments is still one form's bug.
    for index in indices {
        if let Some(sequence) = view.children.get(index) {
            if is_quoted_list_literal(sequence) {
                violations.push(DestructiveLiteralItem {
                    span: view.span,
                    line: line_of(source, view.span.start().get()),
                    operator: head.to_ascii_lowercase(),
                    literal: render_expression(sequence),
                });
                return;
            }
        }
    }
}

/// Collects every destructive call on a quoted list literal in one file, with
/// the number of destructive calls scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "every destructive call here is on a
/// fresh list" for Common Lisp and "nothing was looked for" for Clojure, and
/// the two read identically without the flag.
pub fn collect_destructive_literals(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<DestructiveLiteralItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("destructive_call_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut destructive_call_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_call(
                subview,
                source,
                &mut destructive_call_count,
                &mut violations,
            );
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("destructive_call_count", json!(destructive_call_count))],
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

    fn report(input: &str) -> FileFindings<DestructiveLiteralItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_destructive_literals(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect destructive literals")
    }

    /// The `(destructive_call_count, violations)` pair the report is built from.
    fn calls(input: &str) -> (u64, Vec<DestructiveLiteralItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "destructive_call_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("destructive_call_count in the summary");
        (count, report.findings)
    }

    #[test]
    fn flags_nreverse_of_a_quoted_list() {
        let (count, violations) = calls("(nreverse '(a b c))");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "nreverse");
    }

    #[test]
    fn flags_sort_of_a_quoted_list() {
        let (_, violations) = calls("(sort '(3 1 2) #'<)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "sort");
    }

    #[test]
    fn flags_an_explicit_quote_form() {
        let (_, violations) = calls("(stable-sort (quote (2 1)) #'<)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "stable-sort");
    }

    #[test]
    fn does_not_flag_a_fresh_list() {
        let (count, violations) = calls("(nreverse (list a b c))");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_variable_sequence() {
        let (_, violations) = calls("(sort xs #'<)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_quoted_symbol_or_empty_list() {
        let (_, violations) = calls("(nreverse 'foo)");
        assert!(violations.is_empty());
        let (_, violations) = calls("(nreverse '())");
        assert!(violations.is_empty());
    }

    #[test]
    fn flags_delete_with_a_literal_sequence() {
        // delete's sequence is the second argument.
        let (_, violations) = calls("(delete x '(1 2 3))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "delete");
    }

    #[test]
    fn does_not_flag_delete_of_a_literal_item() {
        // The literal here is the item being deleted, not the sequence.
        let (_, violations) = calls("(delete '(1 2) xs)");
        assert!(violations.is_empty());
    }

    #[test]
    fn flags_nsubstitute_with_a_literal_sequence() {
        // nsubstitute's sequence is the third argument.
        let (_, violations) = calls("(nsubstitute a b '(1 2 3))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "nsubstitute");
    }

    #[test]
    fn flags_a_set_operation_literal_in_either_position() {
        let (_, violations) = calls("(nunion '(1 2) xs)");
        assert_eq!(violations.len(), 1);
        let (_, violations) = calls("(nintersection xs '(3 4))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn flags_rplaca_of_a_literal_cons() {
        let (_, violations) = calls("(rplaca '(1 2) 9)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "rplaca");
    }

    #[test]
    fn flags_a_non_last_nconc_literal_but_not_the_last() {
        // nconc modifies all but the last argument.
        let (_, violations) = calls("(nconc '(1 2) xs)");
        assert_eq!(violations.len(), 1);
        // A literal as the LAST nconc argument is not modified.
        let (_, violations) = calls("(nconc xs '(1 2))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_non_destructive_functions() {
        // reverse (non-destructive) copies, so a literal is fine.
        let (count, violations) = calls("(reverse '(1 2 3))");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_the_head() {
        let (_, violations) = calls("(NREVERSE '(1 2))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_call_nested_in_a_body() {
        let (_, violations) = calls("(defun f () (nbutlast '(1 2 3)))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "nbutlast");
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(nreverse '(1 2))", Dialect::Clojure).expect("parse");
        let report = collect_destructive_literals(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("collect destructive literals");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("destructive_call_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(nreverse xs)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_its_operator_and_its_literal() {
        let report = report("(defun f ()\n  (nbutlast '(1 2 3)))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "destructive-literal");
        assert_eq!(
            finding.json_fields(),
            vec![
                ("operator", json!("nbutlast")),
                ("literal", json!(finding.literal)),
            ]
        );
        assert_eq!(
            finding.text_columns(),
            vec![
                "nbutlast".to_owned(),
                format!("literal={}", finding.literal),
            ]
        );
        assert_eq!(
            finding.message(),
            format!(
                "nbutlast destructively modifies quoted literal {} (undefined behavior)",
                finding.literal
            )
        );
    }

    #[test]
    fn the_summary_counts_every_destructive_call_scanned_not_only_the_flagged_ones() {
        let report = report("(nreverse '(1 2))\n(nreverse xs)\n");
        assert_eq!(report.summary, vec![("destructive_call_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
