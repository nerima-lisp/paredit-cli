//! Common Lisp default-`eql`-search-for-a-literal detection: a sequence search
//! or membership function whose searched item is a *string* or *quoted list*
//! literal and which passes no `:test`. These functions default their test to
//! `eql`, and `eql` compares strings and conses by object identity — two string
//! or list literals are distinct objects — so `(member "x" list)`,
//! `(assoc '(a) alist)`, or `(substitute y "x" seq)` silently never matches,
//! regardless of contents. The fix is an explicit `:test #'equal`
//! (or `#'string=`).
//!
//! The searched item's argument position differs per function — first for
//! `member`/`assoc`/`rassoc`/`find`/`position`/`count`/`remove`/`delete`/
//! `adjoin`/`pushnew`, second (the `old` value) for `substitute`/`nsubstitute`/
//! `subst`/`nsubst`. The item must be a string literal or a quoted, non-empty
//! list literal (numbers, characters, keywords, and symbols are compared
//! correctly by `eql` and are never flagged). A call that already passes
//! `:test` or `:test-not` is left alone.
//!
//! This is the search-function sibling of `eql-string-comparison` and
//! `eql-list-comparison`, which cover the same identity pitfall for `eq`/`eql`.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding, line_of};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::expression_equality::render_expression;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionView, Path as SexprPath, ReaderPrefix, SyntaxTree,
};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, is_paren_list, list_head};
use serde_json::{Value, json};

/// The 0-based index of the searched item argument for a default-`eql` search
/// function, or `None` if the head is not one of them. Required positionals sit
/// at fixed indices (keyword arguments follow), so the position is stable.
fn item_index(head: &str) -> Option<usize> {
    let eq = |name: &str| head.eq_ignore_ascii_case(name);
    if eq("member")
        || eq("assoc")
        || eq("rassoc")
        || eq("find")
        || eq("position")
        || eq("count")
        || eq("remove")
        || eq("delete")
        || eq("adjoin")
        || eq("pushnew")
    {
        Some(1)
    } else if eq("substitute") || eq("nsubstitute") || eq("subst") || eq("nsubst") {
        Some(2)
    } else {
        None
    }
}

/// Whether `view` is a string literal (`"…"`).
fn is_string_literal(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with('"'))
}

/// Whether `view` is a quoted, non-empty list literal — `'(a b)` or
/// `(quote (a b))`. An empty quoted list (`'()`) is `nil` and eql-comparable.
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

/// Whether any argument is a `:test` or `:test-not` keyword.
fn has_explicit_test(view: &ExpressionView) -> bool {
    view.children.iter().skip(1).any(|child| {
        atom_text(child).is_some_and(|text| {
            text.eq_ignore_ascii_case(":test") || text.eq_ignore_ascii_case(":test-not")
        })
    })
}

#[derive(Debug, Clone)]
pub struct EqlSearchLiteralItem {
    pub span: ByteSpan,
    /// The 1-based line the call starts on.
    pub line: usize,
    /// The search function (`member`, `assoc`, …).
    pub operator: String,
    /// A rendered form of the string/list literal being searched for.
    pub literal: String,
}

impl Finding for EqlSearchLiteralItem {
    /// The rule's own name, not the operator.
    ///
    /// The operator is the natural discriminator here, but it is a `String`
    /// read out of the source and `kind` is `&'static str`; the fourteen heads
    /// are not modelled as a closed set anywhere in this module. It stays a
    /// text column and a JSON field instead.
    fn kind(&self) -> &'static str {
        "eql-search-literal"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    /// The old text row's trailing columns: the bare operator, then the
    /// `literal=`-prefixed literal, in that order.
    fn text_columns(&self) -> Vec<String> {
        vec![self.operator.clone(), format!("literal={}", self.literal)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("operator", json!(self.operator)),
            ("literal", json!(self.literal)),
        ]
    }

    /// The same sentence the `eql-search-literal` lint rule writes, so a SARIF
    /// or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!(
            "{} searches for literal {} with the default eql test; add :test",
            self.operator, self.literal
        )
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_call(
    view: &ExpressionView,
    source: &str,
    search_call_count: &mut usize,
    violations: &mut Vec<EqlSearchLiteralItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let Some(index) = item_index(head) else {
        return;
    };
    *search_call_count += 1;

    let Some(item) = view.children.get(index) else {
        return;
    };
    if (is_string_literal(item) || is_quoted_list_literal(item)) && !has_explicit_test(view) {
        violations.push(EqlSearchLiteralItem {
            span: view.span,
            line: line_of(source, view.span.start().get()),
            operator: head.to_ascii_lowercase(),
            literal: render_expression(item),
        });
    }
}

/// Collects every default-eql search for a string/list literal in one file,
/// with the number of search calls scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no default-eql literal search here" for
/// Common Lisp and "nothing was looked for" for Clojure, and the two read
/// identically without the flag.
pub fn build_eql_search_literal_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<EqlSearchLiteralItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("search_call_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut search_call_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_call(subview, source, &mut search_call_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("search_call_count", json!(search_call_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<EqlSearchLiteralItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_eql_search_literal_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build eql search literal report")
    }

    /// The `(search_call_count, violations)` pair the report is built from.
    fn searches(input: &str) -> (u64, Vec<EqlSearchLiteralItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "search_call_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("search_call_count in the summary");
        (count, report.findings)
    }

    #[test]
    fn flags_member_of_a_string_literal() {
        let (count, violations) = searches("(member \"x\" items)");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "member");
    }

    #[test]
    fn flags_assoc_of_a_quoted_list() {
        let (_, violations) = searches("(assoc '(a b) alist)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "assoc");
    }

    #[test]
    fn flags_find_and_position_of_a_string() {
        let (_, violations) = searches("(find \"a\" xs)");
        assert_eq!(violations.len(), 1);
        let (_, violations) = searches("(position \"a\" xs)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_when_test_is_given() {
        let (count, violations) = searches("(member \"x\" items :test #'equal)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_test_not() {
        let (_, violations) = searches("(assoc '(a) alist :test-not #'equal)");
        assert!(violations.is_empty());
    }

    #[test]
    fn flags_substitute_of_a_string_old_value() {
        // substitute searches for its SECOND argument (old).
        let (_, violations) = searches("(substitute new \"x\" seq)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "substitute");
    }

    #[test]
    fn does_not_flag_substitute_new_value_literal() {
        // The literal is the NEW value (index 1), not the searched old value.
        let (_, violations) = searches("(substitute \"n\" old seq)");
        assert!(violations.is_empty());
    }

    #[test]
    fn flags_adjoin_and_pushnew_of_a_string() {
        let (_, violations) = searches("(adjoin \"x\" set)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "adjoin");
        let (_, violations) = searches("(pushnew \"x\" places)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "pushnew");
    }

    #[test]
    fn does_not_flag_a_number_or_keyword_item() {
        // eql compares numbers and keywords correctly.
        let (_, violations) = searches("(member 5 items)");
        assert!(violations.is_empty());
        let (_, violations) = searches("(find :k items)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_variable_item() {
        let (_, violations) = searches("(member x items)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_an_empty_quoted_list() {
        let (_, violations) = searches("(member '() items)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_non_search_functions() {
        let (count, violations) = searches("(list \"x\" items)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_the_head() {
        let (_, violations) = searches("(MEMBER \"x\" items)");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(member \"x\" items)", Dialect::Clojure)
            .expect("parse");
        let report = build_eql_search_literal_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build eql search literal report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("search_call_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(member x items)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_its_operator_and_its_literal() {
        let report = report("(defun f (items)\n  (member \"x\" items))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        // The operator is a source-read String, so the kind stays the rule's
        // own name and the operator rides along beside it.
        assert_eq!(finding.kind(), "eql-search-literal");
        assert_eq!(
            finding.json_fields(),
            vec![("operator", json!("member")), ("literal", json!("\"x\""))]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["member".to_owned(), "literal=\"x\"".to_owned()]
        );
    }

    #[test]
    fn the_summary_counts_every_search_call_not_only_the_flagged_ones() {
        let report = report("(member \"x\" items)\n(find 5 xs)\n(position k xs)\n");
        assert_eq!(report.summary, vec![("search_call_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }
}
