//! Telling a Scheme identifier apart from a literal that also parses as an atom.
//!
//! The parser reads `"text"`, `42`, `#t` and `#\a` as atoms, because at the
//! structural level they are leaves like any other. A binder, though, can only
//! bind an *identifier*, so every layer that reads a name out of a form has to
//! make this distinction or it will happily report `(define "x" 1)` as
//! defining a variable called `"x"`.

use crate::sexpr::{ExpressionKind, ExpressionView};

/// Whether an atom's text is a Scheme identifier rather than a literal.
///
/// Deliberately a syntactic test, not a full R7RS 7.1.1 grammar check. It
/// rejects the literal forms an atom can take and accepts everything else,
/// which keeps the peculiar identifiers -- `+`, `-`, `...`, `->vector`, `1+`
/// in Guile -- that a stricter reading would throw away.
#[must_use]
pub fn is_scheme_identifier_text(text: &str) -> bool {
    let mut characters = text.chars();
    let Some(first) = characters.next() else {
        return false;
    };

    match first {
        // String, character and the `#t`/`#f`/`#(`/`#u8(` family.
        '"' | '#' => false,
        // A lone `.` is the improper-list marker; `...` is the syntax-rules
        // ellipsis, which *is* an identifier.
        '.' => text.chars().any(|character| character != '.'),
        // `|weird sym|` is an identifier by R7RS 2.1, whatever it contains.
        '|' => true,
        _ => !is_number_start(text, first),
    }
}

/// Whether an expression is a plain, unprefixed identifier atom.
#[must_use]
pub fn is_scheme_identifier(view: &ExpressionView) -> bool {
    view.kind == ExpressionKind::Atom
        && view.reader_prefixes.is_empty()
        && view.text.as_deref().is_some_and(is_scheme_identifier_text)
}

/// The identifier text of an expression, or `None` if it is not one.
#[must_use]
pub fn scheme_identifier_text(view: &ExpressionView) -> Option<&str> {
    is_scheme_identifier(view)
        .then_some(view.text.as_deref())
        .flatten()
}

/// Whether the token reads as a number rather than a symbol.
///
/// `+` and `-` alone are identifiers, and so is `->list`; only a sign actually
/// followed by a digit or a decimal point starts a number.
fn is_number_start(text: &str, first: char) -> bool {
    if first.is_ascii_digit() {
        return true;
    }
    if !matches!(first, '+' | '-') {
        return false;
    }
    text[1..]
        .chars()
        .next()
        .is_some_and(|second| second.is_ascii_digit() || second == '.')
}
