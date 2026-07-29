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
use paredit_core_syntax::common_lisp::{
    common_lisp_reader_conditional_kind, common_lisp_reader_label_kind,
};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionView, ReaderPrefix};

use crate::shared::{SemanticFile, line_of, snippet};

const SNIPPET_LIMIT: usize = 64;

/// The [`literal_kind`] tag for a float.
///
/// Named rather than spelled inline because a consumer that *writes* the
/// reader spelling back to source has to recognise floats and refuse them —
/// see [`literal_text`] for why — and a bare `"float"` in the write path
/// would drift silently from the tag this module produces.
pub const FLOAT_LITERAL_KIND: &str = "float";

/// How a folded value is printed.
///
/// [`LiteralValue`] has no `Display`, and the reader spelling is what a
/// consumer needs: a report that says `Integer(3)` cannot be pasted back into
/// source, and `3` can.
///
/// Strings are printed in *Lisp* string syntax, which is not Rust's: a Lisp
/// reader recognises exactly two escapes inside `"…"`, `\\` and `\"`, and
/// takes every other character literally — including a real newline. Printing
/// with `{:?}` would emit Rust's `\n`, which a Lisp reader reads as the letter
/// `n`, silently changing the string's contents.
///
/// Floats are printed with a decimal point so they at least read back as
/// floats, but the spelling is *not* faithful: [`LiteralValue::Float`] carries
/// an `f64` and no exponent marker, so `1.0d0` (a `double-float`) and `1.0` (a
/// `single-float`) are indistinguishable by the time they reach here. That
/// loss is why the write side refuses to fold floats at all; this report is
/// read-only, so an approximate spelling is still informative there.
#[must_use]
pub fn literal_text(value: &LiteralValue) -> String {
    match value {
        LiteralValue::Integer(value) => value.to_string(),
        LiteralValue::Char(value) => format!("#\\{value}"),
        LiteralValue::Keyword(name) => name.to_string(),
        LiteralValue::Boolean(true) => "t".to_owned(),
        LiteralValue::Boolean(false) => "nil".to_owned(),
        LiteralValue::Nil => "nil".to_owned(),
        LiteralValue::Text(text) => lisp_string_text(text.as_str()),
        LiteralValue::Float(value) => lisp_float_text(value.get()),
    }
}

/// A string's contents in Lisp reader syntax.
///
/// Only `\` and `"` are escaped. Every other byte — newline, tab, `\r`, a
/// non-ASCII character — is passed through as itself, because that is what the
/// reader will hand back. This is the same rule in Common Lisp, Emacs Lisp
/// data, Scheme, and Clojure for the two characters it does escape, and none
/// of them require escaping the rest inside a string literal.
fn lisp_string_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for character in text.chars() {
        if character == '\\' || character == '"' {
            out.push('\\');
        }
        out.push(character);
    }
    out.push('"');
    out
}

/// A float's value with a decimal point, so it cannot be read back as an
/// integer.
///
/// `f64::to_string` prints `1.0` as `1`, and `1` is a Lisp *integer*: printing
/// a float that way changes the type of the value the report describes. Rust's
/// `Display` for `f64` never uses scientific notation, and the value layer only
/// admits finite floats, so appending `.0` when no point is present is enough.
fn lisp_float_text(value: f64) -> String {
    let text = value.to_string();
    if text.contains('.') {
        text
    } else {
        format!("{text}.0")
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
        LiteralValue::Float(_) => FLOAT_LITERAL_KIND,
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
///
/// Returning at an unevaluated form is what keeps the findings *correct*.
/// `evaluate_constant` refuses a quoted form, but only by looking at that
/// form's own reader prefixes — so descending into `'(a (+ 1 2))` reaches
/// `(+ 1 2)` with the quote no longer visible and reports it foldable. It is
/// not: that `(+ 1 2)` is a three-element list of data, and rewriting it to
/// `3` changes the program. The quote context has to be carried down, and
/// pruning the subtree is how it is carried here.
fn collect_foldable(
    file: &SemanticFile,
    view: &ExpressionView,
    source: &str,
    found: &mut Vec<FoldableExpression>,
) {
    if opens_unevaluated_context(view) {
        return;
    }

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

/// Whether this form and everything under it denotes itself rather than the
/// value of the code it is written as.
///
/// Mirrors `value::service::folding::is_read_time_or_quoted`, which asks the
/// same question of a single form. The difference is what the answer means:
/// there it suppresses one evaluation, here it suppresses a whole subtree.
///
/// `,` (unquote) inside a quasiquote does return to evaluated context, so
/// `` `(a ,(+ 1 2)) `` could in principle be folded. This deliberately does
/// not do that, and refuses to fold anywhere under a quasiquote. Getting it
/// right needs an unquote *depth* counter — nested quasiquotes re-quote, and a
/// `,` one level too shallow means folding data — and the payoff is folding
/// arithmetic inside a macro template, which is rare. The cost of the bug is a
/// silently rewritten program, so the missed opportunity is the cheaper error.
fn opens_unevaluated_context(view: &ExpressionView) -> bool {
    view.reader_prefixes.iter().any(|prefix| {
        matches!(
            prefix,
            ReaderPrefix::Quote
                | ReaderPrefix::Quasiquote
                | ReaderPrefix::ReadEval
                | ReaderPrefix::ReaderConditional
                | ReaderPrefix::ReaderConditionalSplicing
        )
    }) || common_lisp_reader_conditional_kind(view).is_some()
        || common_lisp_reader_label_kind(view).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_semantics::semantics::value::model::{FloatLiteral, TextLiteral};
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

    #[test]
    fn a_directly_quoted_form_is_not_foldable() {
        assert!(report("(defun f () '(+ 1 2))").foldable.is_empty());
    }

    #[test]
    fn arithmetic_nested_inside_quoted_data_is_not_foldable() {
        // Folding this would rewrite the three-element list `(+ 1 2)` — data,
        // not code — to the number 3.
        let report = report("(defun f () '(a (+ 1 2)))");
        assert!(report.foldable.is_empty(), "{report:?}");
    }

    #[test]
    fn arithmetic_nested_inside_a_quasiquote_is_not_foldable() {
        let report = report("(defmacro g () `(list (+ 5 6)))");
        assert!(report.foldable.is_empty(), "{report:?}");
    }

    #[test]
    fn an_unquote_inside_a_quasiquote_stays_unfolded_by_choice() {
        // Conservative on purpose: `,(+ 1 2)` is evaluated, so folding it
        // would be correct, but telling that apart from `,` at the wrong
        // nesting depth needs a counter this does not keep.
        let report = report("(defmacro g () `(list ,(+ 1 2)))");
        assert!(report.foldable.is_empty(), "{report:?}");
    }

    #[test]
    fn quoting_does_not_suppress_a_fold_in_a_sibling_subtree() {
        let report = report("(defun f () (list '(a (+ 1 2)) (+ 3 4)))");
        assert_eq!(report.foldable.len(), 1, "{report:?}");
        assert_eq!(report.foldable[0].value, "7");
    }

    #[test]
    fn a_read_eval_form_hides_its_whole_subtree() {
        let report = report("(defun f () #.(list (+ 1 2)))");
        assert!(report.foldable.is_empty(), "{report:?}");
    }

    #[test]
    fn a_string_prints_in_lisp_escaping_not_rust_escaping() {
        // Common Lisp reads only `\\` and `\"` inside a string. Rust's `{:?}`
        // would emit `\n` here, which the reader takes as the letter `n`.
        let text = LiteralValue::Text(TextLiteral::new("a\nb\\c\"d\te"));
        assert_eq!(literal_text(&text), "\"a\nb\\\\c\\\"d\te\"");
    }

    #[test]
    fn a_folded_string_round_trips_through_the_reader() {
        let source = format!("(defun f () (if t {} 2))", {
            let text = LiteralValue::Text(TextLiteral::new("line1\nline2"));
            literal_text(&text)
        });
        let report = report(&source);
        assert_eq!(report.foldable.len(), 1, "{report:?}");
        assert_eq!(report.foldable[0].value, "\"line1\nline2\"");
        assert_eq!(report.foldable[0].kind, "string");
    }

    #[test]
    fn a_float_never_prints_as_an_integer() {
        // `1` is a Lisp integer; printing a float that way changes its type.
        assert_eq!(
            literal_text(&LiteralValue::Float(FloatLiteral::new(1.0))),
            "1.0"
        );
        assert_eq!(
            literal_text(&LiteralValue::Float(FloatLiteral::new(1.5))),
            "1.5"
        );
        assert_eq!(
            literal_text(&LiteralValue::Float(FloatLiteral::new(-2.0))),
            "-2.0"
        );
    }

    #[test]
    fn a_float_valued_fold_is_reported_as_a_float() {
        // The report may show it; the write side refuses it, because `1.0d0`
        // is a `double-float` and nothing here still knows that.
        let report = report("(defun f () (if t 1.0d0 2))");
        assert_eq!(report.foldable.len(), 1, "{report:?}");
        assert_eq!(report.foldable[0].kind, FLOAT_LITERAL_KIND);
        assert_eq!(report.foldable[0].value, "1.0");
    }
}
