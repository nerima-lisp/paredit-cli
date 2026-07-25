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
//! [`crate::domain::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, ReaderPrefix, SyntaxTree};
use crate::domain::view_query::{atom_text, for_each_subview, list_head};

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
    pub path: PathBuf,
    pub span: ByteSpan,
    pub literal: String,
    pub kind: &'static str,
}

#[derive(Debug)]
pub struct RedundantQuoteSummary {
    pub quoted_form_count: usize,
    pub violations: Vec<RedundantQuoteItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct RedundantQuotePolicyOptions {
    fail_on_violation: bool,
}

impl RedundantQuotePolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct RedundantQuotePolicy {
    pub fail_on_violation: bool,
    pub quoted_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

fn examine_quote(
    view: &ExpressionView,
    path: &Path,
    quoted_form_count: &mut usize,
    violations: &mut Vec<RedundantQuoteItem>,
) {
    // Reader-sugar quote: `'5`, `':foo`, `'"x"`, `'#\a`.
    if view.reader_prefixes.contains(&ReaderPrefix::Quote) {
        *quoted_form_count += 1;
        if let Some((text, kind)) = quoted_literal_atom(view) {
            violations.push(RedundantQuoteItem {
                path: path.to_path_buf(),
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
            path: path.to_path_buf(),
            span: view.span,
            literal: text.to_owned(),
            kind: kind.describe(),
        });
    }
}

/// Collects every redundant quote of a self-evaluating literal across a whole
/// file, along with the total number of quoted forms scanned.
pub fn collect_redundant_quotes(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<RedundantQuoteItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut quoted_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_quote(subview, path, &mut quoted_form_count, &mut violations)
        });
    }
    Ok((quoted_form_count, violations))
}

pub fn summarize_redundant_quotes(
    quoted_form_count: usize,
    violations: Vec<RedundantQuoteItem>,
) -> RedundantQuoteSummary {
    RedundantQuoteSummary {
        quoted_form_count,
        violations,
    }
}

pub fn evaluate_redundant_quote_policy(
    options: RedundantQuotePolicyOptions,
    summary: &RedundantQuoteSummary,
) -> RedundantQuotePolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    RedundantQuotePolicy {
        fail_on_violation: options.fail_on_violation(),
        quoted_form_count: summary.quoted_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quotes(input: &str) -> (usize, Vec<RedundantQuoteItem>) {
        let tree = SyntaxTree::parse(input).expect("parse input");
        collect_redundant_quotes(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect redundant quotes")
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

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse("(list '5)").expect("parse input");
        let (quoted_form_count, violations) =
            collect_redundant_quotes(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect redundant quotes");
        assert_eq!(quoted_form_count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (quoted_form_count, items) = quotes("(list '5)");
        let summary = summarize_redundant_quotes(quoted_form_count, items);

        let quiet =
            evaluate_redundant_quote_policy(RedundantQuotePolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_redundant_quote_policy(RedundantQuotePolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
