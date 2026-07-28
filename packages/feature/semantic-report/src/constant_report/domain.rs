//! Expressions that provably evaluate to a literal, and the file-level
//! constants they can be evaluated against.
//!
//! The folding service has existed since the value layer landed, consumed only
//! by lint rules asking one-bit questions — "is this divisor zero?", "is this
//! radix ten?". It can answer a much broader one: *which* expressions in this
//! file are computed at run time but need not be. That is a report, and it is
//! also the input a `fold-constants` edit would take.
//!
//! Two things are deliberately excluded from the findings.
//!
//! A literal atom folds to itself, which is true and useless — `1` is not a
//! constant-folding opportunity. Only compound forms are reported.
//!
//! A form nested inside a form that already folded is not reported separately.
//! `(+ 1 (* 2 3))` folds whole; listing `(* 2 3)` again would triple-count the
//! same opportunity and mislead any consumer that sums the findings.

use std::path::PathBuf;

use paredit_core_semantics::semantics::value::{LiteralValue, Value, evaluate_constant};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionView};

use crate::shared::{SemanticFile, line_of, snippet};

const SNIPPET_LIMIT: usize = 64;

/// How a folded value is printed.
///
/// [`LiteralValue`] has no `Display`, and the reader spelling is what a
/// consumer needs: a report that says `Integer(3)` cannot be pasted back into
/// source, and `3` can.
#[must_use]
pub fn literal_text(value: &LiteralValue) -> String {
    match value {
        LiteralValue::Integer(value) => value.to_string(),
        LiteralValue::Char(value) => format!("#\\{value}"),
        LiteralValue::Keyword(name) => name.to_string(),
        LiteralValue::Boolean(true) => "t".to_owned(),
        LiteralValue::Boolean(false) => "nil".to_owned(),
        LiteralValue::Nil => "nil".to_owned(),
        LiteralValue::Text(text) => format!("{:?}", text.as_str()),
        LiteralValue::Float(value) => value.get().to_string(),
    }
}

/// The kind of literal a fold produced, for grouping without parsing the text.
#[must_use]
pub const fn literal_kind(value: &LiteralValue) -> &'static str {
    match value {
        LiteralValue::Integer(_) => "integer",
        LiteralValue::Char(_) => "character",
        LiteralValue::Keyword(_) => "keyword",
        LiteralValue::Boolean(_) => "boolean",
        LiteralValue::Nil => "null",
        LiteralValue::Text(_) => "string",
        LiteralValue::Float(_) => "float",
    }
}

/// One compound form that need not be computed at run time.
#[derive(Debug, Clone, PartialEq)]
pub struct FoldableExpression {
    pub span: ByteSpan,
    pub line: usize,
    /// The form as written, elided if long.
    pub text: String,
    /// What it folds to, in reader spelling.
    pub value: String,
    pub kind: &'static str,
    /// How many bytes the fold would remove. Negative is possible in
    /// principle — folding to a long string — so this is signed rather than
    /// silently saturating at zero.
    pub saved_bytes: i64,
}

/// One `defconstant` this file defines, as the value layer resolved it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileConstant {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConstantReportFile {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub dialect_modelled: bool,
    pub foldable: Vec<FoldableExpression>,
    pub constants: Vec<FileConstant>,
}

#[must_use]
pub fn build_constant_report(file: &SemanticFile) -> ConstantReportFile {
    let source = file.tree.source();
    let mut foldable = Vec::new();
    collect_foldable(file, &file.tree.root_view(), source, &mut foldable);
    foldable.sort_by_key(|item| (item.span.start().get(), item.span.end().get()));

    let mut constants = file
        .values
        .constants()
        .map(|(name, value)| FileConstant {
            name: name.as_str().to_owned(),
            value: literal_text(&LiteralValue::from(value.clone())),
        })
        .collect::<Vec<_>>();
    // Name order, because a constant has no span in this table to sort by.
    constants.sort_by(|left, right| left.name.cmp(&right.name));

    ConstantReportFile {
        path: file.path.clone(),
        dialect: file.dialect,
        dialect_modelled: file.dialect_is_modelled(),
        foldable,
        constants,
    }
}

/// Walks the tree, reporting the outermost form of each folded region.
///
/// Returning rather than recursing past a fold is what keeps the count honest:
/// one opportunity is one finding, however deeply the arithmetic nests.
fn collect_foldable(
    file: &SemanticFile,
    view: &ExpressionView,
    source: &str,
    found: &mut Vec<FoldableExpression>,
) {
    if view.kind == ExpressionKind::List {
        if let Value::Known(value) =
            evaluate_constant(file.dialect, view, &file.bindings, &file.values)
        {
            let text = literal_text(&value);
            let width = view.span.end().get() as i64 - view.span.start().get() as i64;
            found.push(FoldableExpression {
                span: view.span,
                line: line_of(source, view.span.start().get()),
                text: snippet(source, view.span, SNIPPET_LIMIT),
                saved_bytes: width - text.len() as i64,
                kind: literal_kind(&value),
                value: text,
            });
            return;
        }
    }

    for child in &view.children {
        collect_foldable(file, child, source, found);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::sexpr::SyntaxTree;
    use std::path::Path;

    fn report_of(source: &str, dialect: Dialect) -> ConstantReportFile {
        let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
        build_constant_report(&SemanticFile::analyze(Path::new("t.lisp"), dialect, tree))
    }

    fn report(source: &str) -> ConstantReportFile {
        report_of(source, Dialect::CommonLisp)
    }

    #[test]
    fn arithmetic_over_literals_folds() {
        let report = report("(defun f () (+ 1 2))");
        assert_eq!(report.foldable.len(), 1);
        assert_eq!(report.foldable[0].value, "3");
        assert_eq!(report.foldable[0].kind, "integer");
    }

    #[test]
    fn a_nested_fold_is_reported_once_at_its_outermost_form() {
        let report = report("(defun f () (+ 1 (* 2 3)))");
        assert_eq!(report.foldable.len(), 1, "{report:?}");
        assert_eq!(report.foldable[0].value, "7");
    }

    #[test]
    fn a_bare_literal_is_not_a_folding_opportunity() {
        let report = report("(defun f () 1)");
        assert!(report.foldable.is_empty(), "{report:?}");
    }

    #[test]
    fn a_call_the_layer_cannot_evaluate_is_not_reported() {
        let report = report("(defun f (x) (some-macro x))");
        assert!(report.foldable.is_empty(), "{report:?}");
    }

    #[test]
    fn the_saving_is_the_width_the_fold_removes() {
        let report = report("(defun f () (+ 1 2))");
        // "(+ 1 2)" is seven bytes; "3" is one.
        assert_eq!(report.foldable[0].saved_bytes, 6);
    }

    #[test]
    fn a_file_level_constant_is_reported_with_its_value() {
        let report = report("(defconstant +limit+ 10)");
        assert_eq!(
            report.constants,
            vec![FileConstant {
                name: "+LIMIT+".to_owned(),
                value: "10".to_owned(),
            }]
        );
    }

    #[test]
    fn findings_are_sorted_by_source_position() {
        let report = report("(defun f () (list (+ 1 2) (* 3 4) (- 9 1)))");
        let starts = report
            .foldable
            .iter()
            .map(|item| item.span.start().get())
            .collect::<Vec<_>>();
        let mut sorted = starts.clone();
        sorted.sort_unstable();
        assert_eq!(starts, sorted);
        assert_eq!(starts.len(), 3);
    }

    #[test]
    fn an_unmodelled_dialect_says_so_rather_than_reporting_nothing() {
        let report = report_of("(+ 1 2)", Dialect::Clojure);
        assert!(!report.dialect_modelled);
        assert!(report.foldable.is_empty());
    }

    #[test]
    fn every_literal_kind_prints_in_reader_spelling() {
        assert_eq!(literal_text(&LiteralValue::Integer(3)), "3");
        assert_eq!(literal_text(&LiteralValue::Char('a')), "#\\a");
        assert_eq!(literal_text(&LiteralValue::Boolean(true)), "t");
        assert_eq!(literal_text(&LiteralValue::Nil), "nil");
    }
}
