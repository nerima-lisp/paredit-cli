//! Common Lisp quoted-`case`-key detection: a `case`, `ccase`, or `ecase`
//! clause whose key designator is quoted — `(case x ('a 1) …)`. `case` keys
//! are **not** evaluated, and `'a` reads as `(quote a)`, so the key designator
//! `(quote a)` is a *list* of the two keys `quote` and `a`. The clause
//! therefore matches the symbols `quote` or `a`, never the value the author
//! meant — almost always a bug where `(case x (a 1) …)` was intended.
//!
//! Both surface forms are detected: the reader-sugar `'a` (an atom carrying a
//! `Quote` prefix) and the explicit `(quote a)` list, whether the key
//! designator is a single key or one element of a key list (`('a 'b)`).
//!
//! Scoped to `case`/`ccase`/`ecase` — the `eql`-key forms. `typecase`'s clause
//! heads are type specifiers, a different shape, and are not inspected here.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`] and the display rendering
//! from [`paredit_core_syntax::expression_equality`].
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
use paredit_core_syntax::view_query::{for_each_subview, is_paren_list, list_head};
use serde_json::{Value, json};

const CASE_HEADS: [&str; 3] = ["case", "ccase", "ecase"];

/// Whether one key form is quoted — the reader-sugar `'x` (a `Quote` prefix)
/// or the explicit `(quote x)` list.
fn is_quoted_form(view: &ExpressionView) -> bool {
    view.reader_prefixes.contains(&ReaderPrefix::Quote)
        || list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("quote"))
}

/// Whether a clause's key designator is quoted: either the designator itself is
/// a quoted form, or it is a key list with a quoted element.
fn key_designator_is_quoted(key_designator: &ExpressionView) -> bool {
    if is_quoted_form(key_designator) {
        return true;
    }
    is_paren_list(key_designator) && key_designator.children.iter().any(is_quoted_form)
}

#[derive(Debug, Clone)]
pub struct QuotedCaseKeyItem {
    /// The span of the quoted key designator.
    pub span: ByteSpan,
    /// The 1-based line the key designator starts on.
    pub line: usize,
    /// The `case`/`ccase`/`ecase` head, in the source's own casing.
    pub head: String,
    /// The key designator as written.
    pub key: String,
}

impl Finding for QuotedCaseKeyItem {
    /// The rule's own name, not the `case`/`ccase`/`ecase` head.
    ///
    /// The head is kept in the source's casing (`ECASE` stays `ECASE`), which
    /// makes it data rather than a tag: a consumer grouping on `kind` would get
    /// one bucket per spelling. It is published as a JSON field instead.
    fn kind(&self) -> &'static str {
        "quoted-case-key"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("head={}", self.head), format!("key={}", self.key)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("head", json!(self.head)), ("key", json!(self.key))]
    }

    /// The same sentence the `quoted-case-key` lint rule writes, so a SARIF or
    /// JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!(
            "{} key {} is quoted; case keys are not evaluated",
            self.head, self.key
        )
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_case(
    view: &ExpressionView,
    source: &str,
    case_form_count: &mut usize,
    violations: &mut Vec<QuotedCaseKeyItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !CASE_HEADS
        .iter()
        .any(|name| head.eq_ignore_ascii_case(name))
    {
        return;
    }
    // A quoted/quasiquoted case form is data or a template, not a call.
    if !view.reader_prefixes.is_empty() {
        return;
    }
    *case_form_count += 1;

    // The keyform is child 1; clauses start at child 2. A feature-conditional
    // clause reads as an opaque atom (not a list) and is skipped.
    for clause in view.children.iter().skip(2) {
        if !is_paren_list(clause) {
            continue;
        }
        let Some(key_designator) = clause.children.first() else {
            continue;
        };
        if key_designator_is_quoted(key_designator) {
            violations.push(QuotedCaseKeyItem {
                span: key_designator.span,
                line: line_of(source, key_designator.span.start().get()),
                head: head.to_owned(),
                key: render_expression(key_designator),
            });
        }
    }
}

/// Collects every `case`/`ccase`/`ecase` clause with a quoted key designator in
/// one file, with the number of such forms scanned as the denominator beside
/// them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no quoted key here" for Common Lisp and
/// "nothing was looked for" for Clojure, and the two read identically without
/// the flag.
pub fn build_quoted_case_key_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<QuotedCaseKeyItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("case_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut case_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_case(subview, source, &mut case_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("case_form_count", json!(case_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<QuotedCaseKeyItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_quoted_case_key_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build quoted case key report")
    }

    /// The `(case_form_count, violations)` pair the report is built from.
    fn keys(input: &str) -> (u64, Vec<QuotedCaseKeyItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "case_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("case_form_count in the summary");
        (count, report.findings)
    }

    #[test]
    fn flags_a_reader_sugar_quoted_key() {
        let (case_form_count, items) = keys("(case x ('a 1) (b 2))");
        assert_eq!(case_form_count, 1);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].head, "case");
    }

    #[test]
    fn flags_an_explicit_quote_key() {
        let (_, items) = keys("(case x ((quote a) 1))");
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn flags_a_quoted_element_in_a_key_list() {
        let (_, items) = keys("(case x (('a 'b) 1))");
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn does_not_flag_ordinary_keys() {
        let (case_form_count, items) = keys("(case x (a 1) (b 2) (t 3))");
        assert_eq!(case_form_count, 1);
        assert!(items.is_empty());
    }

    #[test]
    fn does_not_flag_a_plain_key_list() {
        let (_, items) = keys("(case x ((a b) 1) (otherwise 2))");
        assert!(items.is_empty());
    }

    #[test]
    fn does_not_flag_a_catch_all() {
        let (_, items) = keys("(case x (a 1) (otherwise 2))");
        assert!(items.is_empty());
    }

    #[test]
    fn flags_an_ecase_quoted_key() {
        let (_, items) = keys("(ecase x ('a 1))");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].head, "ecase");
    }

    #[test]
    fn skips_a_feature_conditional_clause() {
        let (case_form_count, items) = keys("(case x (a 1) #+sbcl ('b 2))");
        assert_eq!(case_form_count, 1);
        assert!(items.is_empty());
    }

    #[test]
    fn skips_a_quoted_case_form() {
        let (case_form_count, items) = keys("(list '(case x ('a 1)))");
        assert_eq!(case_form_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn finds_a_case_nested_in_a_function_body() {
        let (case_form_count, items) = keys("(defun f (x) (case x ('a 1)))");
        assert_eq!(case_form_count, 1);
        assert_eq!(items.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(case x ('a 1))", Dialect::Clojure).expect("parse");
        let report = build_quoted_case_key_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build quoted case key report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("case_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(case x (a 1))").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_its_head_and_its_key() {
        let report = report("(defun f (x)\n  (case x ('a 1)))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "quoted-case-key");
        assert_eq!(
            finding.json_fields(),
            vec![("head", json!("case")), ("key", json!("'a"))]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["head=case".to_owned(), "key='a".to_owned()]
        );
    }

    /// The head keeps the source's casing, which is exactly why it is a JSON
    /// field rather than the `kind` tag.
    #[test]
    fn the_head_is_reported_as_written() {
        let report = report("(ECASE x ('a 1))");
        assert_eq!(report.findings[0].head, "ECASE");
        assert_eq!(report.findings[0].kind(), "quoted-case-key");
    }

    #[test]
    fn the_summary_counts_every_case_scanned_not_only_the_flagged_ones() {
        let report = report("(case x ('a 1))\n(case y (b 2))\n(ecase z ('c 3))\n");
        assert_eq!(report.summary, vec![("case_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 2);
    }
}
