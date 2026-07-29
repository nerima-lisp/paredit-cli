//! Whitespace-insensitive rendering of a form's source text.
//!
//! Two things need "the same form, spelled differently, is the same form":
//! back-references in a pattern (`(eq ?x ?x)` must match `(eq  x   x)`) and
//! stable selector ids, which have to survive a reformat.
//!
//! Normalization is deliberately textual rather than structural. Rebuilding
//! text from the tree would drop comments — which live outside the node tree —
//! and two forms that differ only in a comment are *not* the same form for an
//! id whose job is to point at one of them.

use crate::sexpr::ByteSpan;

/// The source covered by `span`, with every run of whitespace collapsed to a
/// single space and the ends trimmed.
///
/// Whitespace inside a string literal is preserved: `"a  b"` and `"a b"` are
/// different strings, and collapsing them would make a stable id follow the
/// wrong one after an edit.
#[must_use]
pub fn normalized_form_text(source: &str, span: ByteSpan) -> String {
    let text = source.get(span.as_range()).unwrap_or_default();
    let mut output = String::with_capacity(text.len());
    let mut in_string = false;
    let mut escaped = false;
    let mut pending_space = false;

    for character in text.chars() {
        if in_string {
            output.push(character);
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }

        if character.is_whitespace() {
            pending_space = !output.is_empty();
            continue;
        }
        if pending_space {
            output.push(' ');
            pending_space = false;
        }
        output.push(character);
        if character == '"' {
            in_string = true;
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sexpr::{ByteOffset, ByteSpan};

    fn normalize(source: &str) -> String {
        normalized_form_text(
            source,
            ByteSpan::new(ByteOffset::new(0), ByteOffset::new(source.len())),
        )
    }

    #[test]
    fn runs_of_whitespace_collapse_to_one_space() {
        assert_eq!(normalize("(a\n   b\t\tc)"), "(a b c)");
    }

    #[test]
    fn leading_and_trailing_whitespace_is_dropped() {
        assert_eq!(normalize("  (a b)  "), "(a b)");
    }

    #[test]
    fn whitespace_inside_a_string_is_preserved() {
        assert_eq!(
            normalize("(format t  \"a  b\"  x)"),
            "(format t \"a  b\" x)"
        );
    }

    #[test]
    fn an_escaped_quote_does_not_end_the_string() {
        assert_eq!(normalize("(a \"x \\\"  y\"  b)"), "(a \"x \\\"  y\" b)");
    }
}
