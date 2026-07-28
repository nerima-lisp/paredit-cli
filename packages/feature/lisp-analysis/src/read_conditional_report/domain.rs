//! Every `#+`/`#-` in a file, the feature expression it tests, and which
//! feature combinations reach which code.
//!
//! A reader conditional decides, before there is a parse, whether the text
//! after it exists at all. That makes it the one construct in the language that
//! an S-expression analysis cannot reason about by looking at the tree: what
//! the tree contains already depends on the answer.
//!
//! The practical consequences are what this report is for. `#+sbcl` and
//! `#+ccl` branches that are not mirrors of each other are a portability bug
//! that no test run on one implementation can find. A `#-(or a b)` guarding a
//! definition means the file's inventory differs per implementation, so every
//! other report's answer is conditional too. And a feature named once in the
//! whole file is usually a typo — `#+sbcl` beside `#+sbcl-thread` reads fine
//! and does something different.
//!
//! The feature expression is parsed here rather than passed through as text,
//! because `(and sbcl (not win32))` and `sbcl` are different claims and a
//! consumer grouping by the raw string would treat every parenthesized form as
//! its own feature.

use std::collections::BTreeSet;
use std::path::Path;

use paredit_core_syntax::common_lisp::{
    CommonLispReaderConditionalKind, common_lisp_reader_conditional_forms,
};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, SyntaxTree};
use serde_json::{Value, json};

use paredit_core_cli::report::{FileFindings, Finding};

/// One reader conditional.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadConditional {
    /// `#+` includes the guarded datum when the expression holds; `#-` excludes
    /// it.
    pub include: bool,
    /// The feature expression as written, whitespace-collapsed.
    pub feature_expression: String,
    /// Every feature name the expression mentions, in name order and without
    /// the `and`/`or`/`not` operators.
    pub features: Vec<String>,
    /// The whole region the conditional consumes: the dispatch, the feature
    /// expression, and the guarded datum.
    pub span: ByteSpan,
    pub line: usize,
    /// The guarded text, elided. What is actually at stake.
    pub guarded: String,
}

impl Finding for ReadConditional {
    fn kind(&self) -> &'static str {
        if self.include { "include" } else { "exclude" }
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            self.feature_expression.clone(),
            format!("features={}", self.features.join(",")),
            self.guarded.clone(),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("include", json!(self.include)),
            ("feature_expression", json!(self.feature_expression)),
            ("features", json!(self.features)),
            ("guarded", json!(self.guarded)),
        ]
    }
}

const GUARDED_LIMIT: usize = 48;

#[must_use]
pub fn build_read_conditional_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> FileFindings<ReadConditional> {
    let modelled = dialect == Dialect::CommonLisp;
    let source = tree.source();

    let findings = if modelled {
        common_lisp_reader_conditional_forms(tree)
            .into_iter()
            .map(|form| {
                let expression = feature_expression(source, form.dispatch_span, form.span);
                ReadConditional {
                    include: form.kind == CommonLispReaderConditionalKind::Include,
                    features: feature_names(&expression),
                    feature_expression: expression,
                    span: form.span,
                    line: line_of(source, form.span.start().get()),
                    guarded: guarded_text(source, form.dispatch_span, form.span),
                }
            })
            .collect()
    } else {
        Vec::new()
    };

    // Distinct features, and features named exactly once. The second is the
    // number worth looking at: a feature mentioned in one place and nowhere
    // else is usually a misspelling of one mentioned everywhere else.
    let mut seen: Vec<&String> = findings
        .iter()
        .flat_map(|finding: &ReadConditional| finding.features.iter())
        .collect();
    seen.sort();
    let distinct = seen.iter().collect::<BTreeSet<_>>().len();
    let singly_used = seen
        .chunk_by(|left, right| left == right)
        .filter(|run| run.len() == 1)
        .count();

    FileFindings::new(
        path.to_path_buf(),
        dialect,
        modelled,
        findings,
        vec![
            ("distinct_feature_count", json!(distinct)),
            ("singly_used_feature_count", json!(singly_used)),
        ],
    )
}

/// The feature expression text: everything between the dispatch and the
/// guarded datum.
///
/// Read off the source rather than off the tree because in a dialect-aware
/// parse the whole conditional is one opaque atom — there is no child node
/// holding the expression to ask.
fn feature_expression(source: &str, dispatch: ByteSpan, whole: ByteSpan) -> String {
    let rest = source
        .get(dispatch.end().get()..whole.end().get())
        .unwrap_or_default()
        .trim_start();
    let end = expression_end(rest);
    rest[..end].split_whitespace().collect::<Vec<_>>().join(" ")
}

fn guarded_text(source: &str, dispatch: ByteSpan, whole: ByteSpan) -> String {
    let rest = source
        .get(dispatch.end().get()..whole.end().get())
        .unwrap_or_default()
        .trim_start();
    let text = rest[expression_end(rest)..]
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if text.chars().count() <= GUARDED_LIMIT {
        return text;
    }
    let head = text
        .chars()
        .take(GUARDED_LIMIT.saturating_sub(1))
        .collect::<String>();
    format!("{head}…")
}

/// How many bytes of `text` the first datum occupies.
///
/// A feature expression is either one symbol or one parenthesized form, so
/// this is depth counting rather than parsing. Unbalanced input returns the
/// whole slice, which yields a visibly odd expression instead of a panic.
fn expression_end(text: &str) -> usize {
    if !text.starts_with('(') {
        return text
            .find(|character: char| character.is_whitespace())
            .unwrap_or(text.len());
    }
    let mut depth = 0usize;
    for (index, character) in text.char_indices() {
        match character {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return index + 1;
                }
            }
            _ => {}
        }
    }
    text.len()
}

/// The feature names an expression mentions, with the boolean operators
/// removed and duplicates collapsed.
fn feature_names(expression: &str) -> Vec<String> {
    let mut names = expression
        .replace(['(', ')'], " ")
        .split_whitespace()
        .filter(|token| !matches!(token.to_ascii_lowercase().as_str(), "and" | "or" | "not"))
        .map(|token| token.to_ascii_uppercase())
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names
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

    fn report(source: &str) -> FileFindings<ReadConditional> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        build_read_conditional_report(Path::new("t.lisp"), Dialect::CommonLisp, &tree)
    }

    #[test]
    fn a_simple_include_names_its_feature() {
        let report = report("#+sbcl (defun f () 1)");
        assert_eq!(report.findings.len(), 1);
        let finding = &report.findings[0];
        assert!(finding.include);
        assert_eq!(finding.feature_expression, "sbcl");
        assert_eq!(finding.features, vec!["SBCL".to_owned()]);
    }

    #[test]
    fn an_exclude_is_distinguished_from_an_include() {
        let report = report("#-sbcl (defun f () 1)");
        assert!(!report.findings[0].include);
        assert_eq!(report.findings[0].kind(), "exclude");
    }

    #[test]
    fn a_compound_expression_reports_every_feature_without_its_operators() {
        let report = report("#+(and sbcl (not win32)) (defun f () 1)");
        assert_eq!(
            report.findings[0].features,
            vec!["SBCL".to_owned(), "WIN32".to_owned()]
        );
        assert_eq!(
            report.findings[0].feature_expression,
            "(and sbcl (not win32))"
        );
    }

    #[test]
    fn the_guarded_datum_is_reported_beside_the_expression() {
        let report = report("#+sbcl (defun f () 1)");
        assert_eq!(report.findings[0].guarded, "(defun f () 1)");
    }

    #[test]
    fn a_feature_named_once_is_counted_as_singly_used() {
        let report = report("#+sbcl (a)\n#+sbcl (b)\n#+ccl (c)");
        assert_eq!(
            report.summary,
            vec![
                ("distinct_feature_count", json!(2)),
                ("singly_used_feature_count", json!(1)),
            ]
        );
    }

    #[test]
    fn findings_are_in_source_order() {
        let report = report("#+a (x)\n#+b (y)\n#+c (z)");
        let starts = report
            .findings
            .iter()
            .map(|finding| finding.span.start().get())
            .collect::<Vec<_>>();
        let mut sorted = starts.clone();
        sorted.sort_unstable();
        assert_eq!(starts, sorted);
        assert_eq!(starts.len(), 3);
    }

    #[test]
    fn a_file_with_no_reader_conditional_reports_nothing() {
        let report = report("(defun f () 1)");
        assert!(report.findings.is_empty());
    }

    #[test]
    fn an_unmodelled_dialect_says_so_rather_than_reporting_nothing() {
        let source = "(defn f [] 1)";
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::Clojure).expect("parse");
        let report = build_read_conditional_report(Path::new("t.clj"), Dialect::Clojure, &tree);
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn an_unbalanced_expression_yields_a_finding_rather_than_a_panic() {
        assert_eq!(expression_end("(and sbcl"), "(and sbcl".len());
    }
}
