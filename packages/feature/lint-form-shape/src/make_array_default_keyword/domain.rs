//! Common Lisp redundant-`make-array`-default-keyword detection: a `make-array`
//! call with an explicit `:adjustable nil` or `:fill-pointer nil`. CLHS specifies
//! that both keywords *default to* `nil` (an ordinary, non-adjustable array with
//! no fill pointer), so `(make-array n :adjustable nil)` is exactly
//! `(make-array n)`. The explicit `nil` restates the default and adds only noise.
//!
//! Scope is limited to `:adjustable` and `:fill-pointer` — the two `make-array`
//! keywords that both (a) default to `nil` unconditionally and (b) are
//! independent of every other keyword. `:displaced-to` also defaults to `nil`
//! but interacts with `:displaced-index-offset`, and `:initial-element nil` is
//! *not* redundant for `make-array` (an unspecified element type leaves contents
//! undefined), so neither is touched.
//!
//! Only a bare `nil` literal value is flagged. A non-`nil` value
//! (`:adjustable t`) is meaningful and left alone. The first matching redundant
//! keyword pair in the call is reported.
//!
//! The fix deletes the redundant ` :adjustable nil` / ` :fill-pointer nil`
//! argument pair (from the end of the preceding argument through the `nil`),
//! leaving the rest byte-identical, so the rule is auto-fixable.
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

/// The `make-array` keywords that unconditionally default to `nil` and are
/// independent of the other keyword arguments.
const NIL_KEYWORDS: [&str; 2] = [":adjustable", ":fill-pointer"];

/// Whether `view` is one of the redundant-when-nil keyword atoms.
fn is_nil_default_keyword(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view)
            .is_some_and(|text| NIL_KEYWORDS.iter().any(|kw| text.eq_ignore_ascii_case(kw)))
}

/// Whether `view` is the bare `nil` literal (no reader prefixes).
fn is_nil_literal(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|t| t.eq_ignore_ascii_case("nil"))
}

#[derive(Debug, Clone)]
pub struct MakeArrayDefaultKeywordItem {
    /// The span of the whole `(make-array …)` call form.
    pub span: ByteSpan,
    /// The span to delete: the ` :adjustable nil` / ` :fill-pointer nil` pair.
    ///
    /// The rewrite's input, but the old report published it, so it stays on the
    /// report too: a consumer applying the edit itself needs the same bytes the
    /// fix does.
    pub removal_span: ByteSpan,
    /// The keyword name, for the finding message.
    pub keyword: String,
}

impl Finding for MakeArrayDefaultKeywordItem {
    /// The rule's name, not the keyword: `keyword` is source text carried per
    /// finding (case as written), and `kind` is a fixed vocabulary the interop
    /// formats turn into a rule id. It stays a `json_fields` entry and the lone
    /// text column, where filtering on it still works.
    fn kind(&self) -> &'static str {
        "make-array-default-keyword"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    /// The keyword, bare — the prefixing style the old text row used.
    fn text_columns(&self) -> Vec<String> {
        vec![self.keyword.clone()]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("keyword", json!(self.keyword)),
            (
                "removal_span",
                json!({
                    "start": self.removal_span.start().get(),
                    "end": self.removal_span.end().get(),
                }),
            ),
        ]
    }

    /// The same sentence the `make-array-default-keyword` lint rule writes, so
    /// a SARIF or JUnit consumer reading both sees one finding described one
    /// way.
    fn message(&self) -> String {
        format!(
            "explicit {} nil restates make-array's default; drop it",
            self.keyword
        )
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    call_form_count: &mut usize,
    violations: &mut Vec<MakeArrayDefaultKeywordItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("make-array") {
        return;
    }
    *call_form_count += 1;

    for index in 1..view.children.len().saturating_sub(1) {
        if !is_nil_default_keyword(&view.children[index]) {
            continue;
        }
        if !is_nil_literal(&view.children[index + 1]) {
            continue;
        }
        let keyword = atom_text(&view.children[index])
            .unwrap_or_default()
            .to_owned();
        let removal_span = ByteSpan::new(
            view.children[index - 1].span.end(),
            view.children[index + 1].span.end(),
        );
        violations.push(MakeArrayDefaultKeywordItem {
            span: view.span,
            removal_span,
            keyword,
        });
        return;
    }
}

/// Collects every `make-array` call in one file with a redundant
/// `:adjustable nil` / `:fill-pointer nil`, with the number of `make-array`
/// calls scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no restated default here" for Common
/// Lisp and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_make_array_default_keyword_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<MakeArrayDefaultKeywordItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("call_form_count", json!(0))],
        ));
    }

    let mut call_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, &mut call_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("call_form_count", json!(call_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<MakeArrayDefaultKeywordItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_make_array_default_keyword_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build make-array default keyword report")
    }

    /// The `(call_form_count, violations)` pair the report is built from.
    fn calls(input: &str) -> (u64, Vec<MakeArrayDefaultKeywordItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "call_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("call_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_adjustable_nil() {
        let source = "(make-array n :adjustable nil)";
        let (count, violations) = calls(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(
            slice(source, violations[0].removal_span),
            " :adjustable nil"
        );
    }

    #[test]
    fn flags_fill_pointer_nil() {
        let source = "(make-array n :fill-pointer nil)";
        let (_, violations) = calls(source);
        assert_eq!(
            slice(source, violations[0].removal_span),
            " :fill-pointer nil"
        );
    }

    #[test]
    fn removal_keeps_other_keywords() {
        let source = "(make-array n :adjustable nil :element-type 'bit)";
        let (_, violations) = calls(source);
        assert_eq!(
            slice(source, violations[0].removal_span),
            " :adjustable nil"
        );
    }

    #[test]
    fn does_not_flag_non_nil() {
        let (count, violations) = calls("(make-array n :adjustable t)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_initial_element_nil() {
        // :initial-element nil is NOT redundant for make-array.
        assert!(calls("(make-array n :initial-element nil)").1.is_empty());
    }

    #[test]
    fn case_folds_head_and_keyword() {
        let (_, violations) = calls("(MAKE-ARRAY n :ADJUSTABLE nil)");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(make-array n :adjustable nil)", Dialect::Clojure)
                .expect("parse");
        let report =
            build_make_array_default_keyword_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build make-array default keyword report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("call_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(make-array n)").dialect_modelled);
    }

    /// `keyword` and `removal_span` were both on the old JSON, so both stay.
    #[test]
    fn a_finding_carries_its_line_its_keyword_and_its_removal_span() {
        let report = report("(defun f (n)\n  (make-array n :adjustable nil))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "make-array-default-keyword");
        assert_eq!(finding.text_columns(), vec![":adjustable".to_owned()]);
        assert_eq!(
            finding.json_fields(),
            vec![
                ("keyword", json!(":adjustable")),
                (
                    "removal_span",
                    json!({
                        "start": finding.removal_span.start().get(),
                        "end": finding.removal_span.end().get(),
                    })
                ),
            ]
        );
    }

    #[test]
    fn the_summary_counts_every_call_scanned_not_only_the_flagged_ones() {
        let report = report("(make-array n :adjustable nil)\n(make-array n)\n(make-array n t)\n");
        assert_eq!(report.summary, vec![("call_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }
}
