//! A docstring whose *first line* — the line every doc generator shows on its
//! own — is too wide to read as a summary.
//!
//! Common Lisp's `documentation` returns the whole string, but everything that
//! presents it in a list presents the first line: `apropos`, an editor's
//! echo-area hint, a generated API index. A first line of 200 characters is
//! not a summary, it is the whole explanation with the line breaks missing, and
//! it is shown truncated wherever it matters.
//!
//! # What this is not
//!
//! Not a docstring/parameter agreement check. That already exists, twice over,
//! in `paredit-feature-code-metrics`'s `docstring_report`
//! (`DocstringIssue::StaleParameter` and `UndocumentedParameter`).
//!
//! Not a line-length rule for source lines. It measures the docstring's own
//! text, after escape resolution, so `\"` counts as the one character a reader
//! sees rather than the two the source spells.
//!
//! # Limits, deliberately
//!
//! - **A default that does not nag.** [`MAX_WIDTH`] defaults to 110, which is
//!   well past any ordinary one-line docstring; the measured maximum over this
//!   repository's own Lisp sources is far below it. A project that wants the
//!   conventional 80 sets `--rule-arg docstring-summary-line-too-long.max=80`.
//!   The rule is also tagged `pedantic`: how wide a summary may be is a house
//!   style, not a defect.
//! - **An unbreakable first line is not reported.** A summary with no
//!   whitespace in it — a URL, one very long symbol — cannot be shortened by
//!   wrapping, so complaining about it asks for something impossible.
//! - **Only `defun`/`defmacro`/`defmethod` and the three variable forms.** A
//!   `defclass`'s `(:documentation …)` and a `defstruct`'s docstring slot are
//!   not read; `defstruct`'s position collides with a slot name, and reporting
//!   a *slot* as an over-long summary would be worse than saying nothing.
//! - **A lone string body is a return value.** `(defun greeting () "…")`
//!   returns its string and is never measured.

use paredit_core_lint_engine::model::RuleSetting;
use paredit_core_syntax::definition::definition_shape;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{is_paren_list, list_head, unqualified};

use crate::support::{
    docstring_place, docstring_view_of, has_child_string_literal, string_literal_text, summary_line,
};

/// The knob: how wide a docstring's first line may be before this rule speaks.
///
/// 110 rather than the conventional 80 on purpose. This rule's failure mode is
/// nagging on documentation that is already fine, and a default that fires on
/// ordinary prose gets the whole rule switched off — taking the genuinely
/// runaway 200-character one-liners with it. A project with a house limit sets
/// one.
pub const MAX_WIDTH: RuleSetting = RuleSetting::new(
    "max",
    110,
    "how wide a docstring's first line may be, in characters, before it is reported",
);

/// One docstring whose summary line is wider than the limit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverLongSummary {
    /// The docstring literal's own span, not the definition's: the text this
    /// is about is the thing to point at.
    pub span: ByteSpan,
    /// The definition's name.
    pub name: String,
    /// The defining form, lowercased and unqualified (`defun`, `defvar`, …).
    pub form: String,
    /// The measured width of the first line, in characters.
    pub width: usize,
    /// The limit it exceeded.
    pub limit: usize,
}

/// Examines one definition and reports it if its docstring's first line is
/// wider than `limit`.
///
/// `limit` is passed in rather than read from a context so the detection can be
/// tested at every threshold without building an engine pass.
#[must_use]
pub fn examine(view: &ExpressionView, limit: usize) -> Option<OverLongSummary> {
    if !is_paren_list(view) {
        return None;
    }
    let head = list_head(view)?;
    let place = docstring_place(head)?;
    // The whole rule, decided on raw source bytes before anything is built.
    // The width compared below is a *character* count of a prefix of the
    // docstring's unescaped text, and for UTF-8 no string has more characters
    // than bytes; unescaping only ever shortens, and the delimiting quotes are
    // counted here and not there. So a literal no longer than the limit cannot
    // hold a summary line past it.
    //
    // Asked of every direct child rather than of the docstring, because
    // finding *the* docstring means building a `DefinitionShape` first — and
    // the docstring is always a direct child, so a definition with no
    // long-enough child literal has no long-enough docstring either.
    //
    // This is why the rule costs nothing on `clean/forms/*`: an ordinary
    // one-line docstring loses here, on a length comparison, before a shape, an
    // unescaped copy, or an owned name is built.
    if !has_child_string_literal(view, |literal| literal.len() > limit) {
        return None;
    }

    let shape = definition_shape(Dialect::CommonLisp, view, head)?;
    let docstring = docstring_view_of(shape, place, view)?;
    let text = string_literal_text(docstring)?;
    let summary = summary_line(&text);

    // An unbreakable line cannot be wrapped, so reporting it asks for
    // something the author cannot do.
    if !summary.chars().any(char::is_whitespace) {
        return None;
    }

    let width = summary.chars().count();
    if width <= limit {
        return None;
    }
    Some(OverLongSummary {
        span: docstring.span,
        name: shape.name(view)?.to_owned(),
        form: unqualified(head).to_ascii_lowercase(),
        width,
        limit,
    })
}

impl OverLongSummary {
    /// The sentence the rule reports.
    #[must_use]
    pub fn message(&self) -> String {
        format!(
            "{} {}'s docstring opens with a {}-character line, past the {}-character summary \
             limit; doc generators show that line on its own",
            self.form, self.name, self.width, self.limit
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::sexpr::SyntaxTree;

    /// The default limit, so the tests that matter most are the ones a user
    /// meets. Pinned against the declaration rather than read from it, so
    /// changing the default has to come past these tests.
    const DEFAULT: usize = 110;

    #[test]
    fn the_pinned_default_is_the_declared_one() {
        assert_eq!(MAX_WIDTH.default(), 110);
        assert_eq!(MAX_WIDTH.key(), "max");
    }

    fn found(source: &str, limit: usize) -> Option<OverLongSummary> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        let form = tree.root_view().children.first()?.clone();
        examine(&form, limit)
    }

    fn summary_of(width: usize) -> String {
        // Words, so the whitespace guard is not what is under test.
        let mut text = String::new();
        while text.chars().count() < width {
            text.push_str("ab ");
        }
        text.truncate(width);
        text
    }

    // --- positive

    #[test]
    fn flags_a_docstring_whose_first_line_is_wider_than_the_limit() {
        let source = format!("(defun f (x) \"{}\" (+ x 1))", summary_of(40));
        let item = found(&source, 20).expect("a finding");
        assert_eq!(item.name, "f");
        assert_eq!(item.form, "defun");
        assert_eq!(item.width, 40);
        assert_eq!(item.limit, 20);
    }

    #[test]
    fn measures_only_the_first_line_of_a_multi_line_docstring() {
        // The first line is short; the rest is long. Nothing is reported.
        let long = summary_of(200);
        let source = format!("(defun f (x) \"Adds one.\n{long}\" (+ x 1))");
        assert_eq!(found(&source, 30), None);

        // Now the first line itself is long.
        let source = format!("(defun f (x) \"{}\nShort.\" (+ x 1))", summary_of(40));
        assert_eq!(found(&source, 30).expect("a finding").width, 40);
    }

    #[test]
    fn flags_a_macro_a_method_and_a_variable_the_same_way() {
        let long = summary_of(40);
        for source in [
            format!("(defmacro m (x) \"{long}\" x)"),
            format!("(defmethod area ((s square)) \"{long}\" 1)"),
            format!("(defparameter *timeout* 30 \"{long}\")"),
            format!("(defvar *cache* nil \"{long}\")"),
            format!("(defconstant +limit+ 10 \"{long}\")"),
        ] {
            assert!(found(&source, 20).is_some(), "not reported: {source}");
        }
    }

    #[test]
    fn the_span_points_at_the_docstring_and_not_at_the_definition() {
        let source = format!("(defun f (x) \"{}\" (+ x 1))", summary_of(40));
        let item = found(&source, 20).expect("a finding");
        let tree = SyntaxTree::parse_with_dialect(&source, Dialect::CommonLisp).expect("parse");
        let form = &tree.root_view().children[0];
        assert!(item.span.start().get() > form.span.start().get());
        assert!(item.span.end().get() < form.span.end().get());
    }

    // --- near-miss negatives

    #[test]
    fn a_line_exactly_at_the_limit_is_not_reported() {
        let source = format!("(defun f (x) \"{}\" (+ x 1))", summary_of(20));
        assert_eq!(found(&source, 20), None);
    }

    #[test]
    fn a_line_one_character_past_the_limit_is_reported() {
        let source = format!("(defun f (x) \"{}\" (+ x 1))", summary_of(21));
        assert_eq!(found(&source, 20).expect("a finding").width, 21);
    }

    /// The rule's headline false positive: the ordinary one-line docstring that
    /// every well-documented function carries.
    #[test]
    fn an_ordinary_one_line_docstring_is_not_reported_at_the_default_limit() {
        for source in [
            "(defun add (x y) \"Return the sum of X and Y.\" (+ x y))",
            "(defun retry (n thunk) \"Attempt THUNK up to N times, returning its first \
             successful value.\" (funcall thunk))",
            "(defparameter *timeout* 30 \"Seconds to wait for the server before giving up.\")",
        ] {
            assert_eq!(found(source, DEFAULT), None, "wrongly reported: {source}");
        }
    }

    #[test]
    fn a_definition_with_no_docstring_is_not_reported() {
        assert_eq!(found("(defun f (x) (+ x 1))", 5), None);
        assert_eq!(found("(defparameter *timeout* 30)", 5), None);
    }

    /// A lone string body is the function's return value.
    #[test]
    fn a_lone_string_body_is_never_measured() {
        let source = format!("(defun greeting () \"{}\")", summary_of(200));
        assert_eq!(found(&source, 20), None);
    }

    /// A summary with nothing to wrap at cannot be shortened.
    #[test]
    fn an_unbreakable_first_line_is_not_reported() {
        let source = format!(
            "(defun f (x) \"https://example.com/{}\" (+ x 1))",
            "a".repeat(200)
        );
        assert_eq!(found(&source, 20), None);
    }

    #[test]
    fn a_form_that_is_not_a_definition_is_not_reported() {
        let source = format!("(let ((x \"{}\")) x)", summary_of(200));
        assert_eq!(found(&source, 20), None);
        assert_eq!(found("(defclass c () ())", 5), None);
    }

    // --- the string-literal negative

    /// A string that merely *contains* a long line is not a docstring unless it
    /// sits in a docstring position.
    #[test]
    fn a_long_string_in_an_argument_position_is_not_a_docstring() {
        let long = summary_of(200);
        let source = format!("(defun f (x) \"Short.\" (format nil \"{long}\") x)");
        assert_eq!(found(&source, 30), None);
    }

    // --- escapes and width

    #[test]
    fn an_escaped_quote_counts_as_one_character_not_two() {
        // 19 source characters between the delimiters, 18 to a reader.
        let source = "(defun f (x) \"aa \\\"bb\\\" cc dd ee\" (+ x 1))";
        let item = found(source, 10).expect("a finding");
        assert_eq!(item.width, "aa \"bb\" cc dd ee".chars().count());
    }

    #[test]
    fn width_is_counted_in_characters_and_not_in_bytes() {
        // Six characters, twelve bytes. The limit is five.
        let source = "(defun f (x) \"日本 語です\" (+ x 1))";
        let item = found(source, 5).expect("a finding");
        assert_eq!(item.width, 6);
    }

    #[test]
    fn the_message_names_the_form_the_definition_and_both_widths() {
        let source = format!("(defun f (x) \"{}\" (+ x 1))", summary_of(40));
        let message = found(&source, 20).expect("a finding").message();
        assert!(message.contains("defun f"), "{message}");
        assert!(message.contains("40-character"), "{message}");
        assert!(message.contains("20-character"), "{message}");
    }
}
