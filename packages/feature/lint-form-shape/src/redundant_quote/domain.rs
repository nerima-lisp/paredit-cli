//! Common Lisp redundant-quote detection: a `quote` (the `'` reader prefix or
//! the explicit `(quote …)` form) applied to a *self-evaluating literal* —
//! `'5`, `'3.14`, `':keyword`, `'"a string"`, `'#\a`. Quoting a
//! self-evaluating object is a no-op: `'5` reads as the very same object as
//! `5`, so the quote is pure noise (and often a sign the author confused
//! quoting with something meaningful).
//!
//! Only the four unambiguously self-evaluating literal categories are flagged:
//! numbers, strings, characters, and keywords. Symbols quoted for their name
//! (`'foo`) are a correct, ubiquitous idiom and are never flagged. `'t` and
//! `'nil` are left alone (their quoted spelling is a matter of taste), and the
//! empty quoted list `'()` is a `nil` *list* literal — not an atom — so it
//! never reaches the atom-gated check. Both the reader-sugar form (`'5`) and
//! the explicit `(quote 5)` form are recognized.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionView, Path as SexprPath, ReaderPrefix, SyntaxTree,
};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};
use serde_json::{Value, json};

/// The self-evaluating literal categories for which a `quote` is redundant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LiteralKind {
    Number,
    String,
    Character,
    Keyword,
}

impl LiteralKind {
    const fn describe(self) -> &'static str {
        match self {
            Self::Number => "number",
            Self::String => "string",
            Self::Character => "character",
            Self::Keyword => "keyword",
        }
    }
}

/// Classifies an atom's text as a self-evaluating literal, or `None` for a
/// symbol (including `t`/`nil`) or anything else whose quoting is defensible.
fn self_evaluating_literal(text: &str) -> Option<LiteralKind> {
    if text.starts_with('"') {
        return Some(LiteralKind::String);
    }
    if text.starts_with("#\\") {
        return Some(LiteralKind::Character);
    }
    if is_keyword(text) {
        return Some(LiteralKind::Keyword);
    }
    if is_number_literal(text) {
        return Some(LiteralKind::Number);
    }
    None
}

/// An atom's own symbol text with any reader-prefix source (e.g. the leading
/// `'` of `'5`) stripped off. `ExpressionView::text` for a prefixed atom
/// includes the prefix spelling; `symbol_offset` is the byte offset to where
/// the symbol content actually begins.
fn atom_content(view: &ExpressionView) -> Option<&str> {
    let text = atom_text(view)?;
    text.get(view.symbol_offset..)
}

fn is_keyword(text: &str) -> bool {
    text.starts_with(':') && text.len() > 1 && !text[1..].contains(':')
}

fn is_number_literal(text: &str) -> bool {
    text.starts_with(|character: char| {
        character.is_ascii_digit() || matches!(character, '+' | '-' | '.')
    }) && (text.parse::<i64>().is_ok() || text.parse::<f64>().is_ok())
}

/// The atom text of a self-evaluating literal quoted with exactly one `Quote`
/// reader prefix — i.e. the `'5` reader-sugar form. A view carrying more than
/// one prefix (`''5`, `` `'5 ``) has different semantics and is not reported.
fn quoted_literal_atom(view: &ExpressionView) -> Option<(&str, LiteralKind)> {
    if view.reader_prefixes.len() != 1 || view.reader_prefixes[0] != ReaderPrefix::Quote {
        return None;
    }
    let text = atom_content(view)?;
    self_evaluating_literal(text).map(|kind| (text, kind))
}

/// The atom text of a self-evaluating literal wrapped in an explicit
/// `(quote X)` form (exactly two children: `quote` and the literal).
fn explicit_quote_literal(view: &ExpressionView) -> Option<(&str, LiteralKind)> {
    if !list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("quote")) {
        return None;
    }
    if view.children.len() != 2 {
        return None;
    }
    let text = atom_content(&view.children[1])?;
    self_evaluating_literal(text).map(|kind| (text, kind))
}

#[derive(Debug, Clone)]
pub struct RedundantQuoteItem {
    pub span: ByteSpan,
    pub literal: String,
    /// Which self-evaluating category the literal falls in, from
    /// `LiteralKind::describe`'s closed set.
    pub kind: &'static str,
}

impl Finding for RedundantQuoteItem {
    /// The literal's category, so `number` and `string` are separable without
    /// parsing JSON. A closed set of four, already canonical here.
    fn kind(&self) -> &'static str {
        self.kind
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    /// The category is already the leading `kind` column, so only the literal
    /// itself is left to say.
    fn text_columns(&self) -> Vec<String> {
        vec![format!("literal={}", self.literal)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("literal", json!(self.literal))]
    }

    /// The same sentence the `redundant-quote` lint rule writes, so a SARIF or
    /// JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!("quoting {} {} is redundant", self.kind, self.literal)
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_quote(
    view: &ExpressionView,
    quoted_form_count: &mut usize,
    violations: &mut Vec<RedundantQuoteItem>,
) {
    // Reader-sugar quote: `'5`, `':foo`, `'"x"`, `'#\a`.
    if view.reader_prefixes.contains(&ReaderPrefix::Quote) {
        *quoted_form_count += 1;
        if let Some((text, kind)) = quoted_literal_atom(view) {
            violations.push(RedundantQuoteItem {
                span: view.span,
                literal: text.to_owned(),
                kind: kind.describe(),
            });
        }
    }

    // Explicit `(quote X)` form.
    if let Some((text, kind)) = explicit_quote_literal(view) {
        *quoted_form_count += 1;
        violations.push(RedundantQuoteItem {
            span: view.span,
            literal: text.to_owned(),
            kind: kind.describe(),
        });
    }
}

/// Collects every redundant quote of a self-evaluating literal in one file,
/// with the number of quoted forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no redundant quote here" for Common Lisp
/// and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_redundant_quote_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<RedundantQuoteItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("quoted_form_count", json!(0))],
        ));
    }

    let mut quoted_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_quote(subview, &mut quoted_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("quoted_form_count", json!(quoted_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<RedundantQuoteItem> {
        let tree = SyntaxTree::parse(input).expect("parse input");
        build_redundant_quote_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build redundant quote report")
    }

    /// The `(quoted_form_count, violations)` pair the report is built from.
    fn quotes(input: &str) -> (u64, Vec<RedundantQuoteItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "quoted_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("quoted_form_count in the summary");
        (count, report.findings)
    }

    #[test]
    fn flags_a_quoted_number() {
        let (_, violations) = quotes("(defparameter *n* '5)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].literal, "5");
        assert_eq!(violations[0].kind, "number");
    }

    #[test]
    fn flags_a_quoted_keyword() {
        let (_, violations) = quotes("(list ':foo)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].literal, ":foo");
        assert_eq!(violations[0].kind, "keyword");
    }

    #[test]
    fn flags_a_quoted_string() {
        let (_, violations) = quotes("(princ '\"hello\")");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].kind, "string");
    }

    #[test]
    fn flags_a_quoted_character() {
        let (_, violations) = quotes("(list '#\\a)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].kind, "character");
    }

    #[test]
    fn flags_an_explicit_quote_of_a_number() {
        let (_, violations) = quotes("(quote 42)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].literal, "42");
        assert_eq!(violations[0].kind, "number");
    }

    #[test]
    fn does_not_flag_a_quoted_symbol() {
        let (quoted_form_count, violations) = quotes("(eq x 'foo)");
        assert_eq!(quoted_form_count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_quoted_t_or_nil() {
        let (_, violations) = quotes("(list 't 'nil)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_the_empty_quoted_list() {
        let (_, violations) = quotes("(list '())");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_quoted_nonempty_list() {
        let (_, violations) = quotes("(member x '(1 2 3))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_double_quoted_literal() {
        // ''5 is (quote (quote 5)) — a genuine two-element list, not redundant.
        let (_, violations) = quotes("(list ''5)");
        assert!(violations.is_empty());
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse("(list '5)").expect("parse input");
        let report = build_redundant_quote_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build redundant quote report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("quoted_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(list 'foo)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_literal() {
        let report = report("(defparameter *n*\n  '5)\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "number");
        assert_eq!(finding.json_fields(), vec![("literal", json!("5"))]);
        assert_eq!(finding.text_columns(), vec!["literal=5".to_owned()]);
    }

    #[test]
    fn the_summary_counts_every_quote_scanned_not_only_the_flagged_ones() {
        let report = report("(list '5 'foo ':k)\n");
        assert_eq!(report.summary, vec![("quoted_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 2);
    }
}
