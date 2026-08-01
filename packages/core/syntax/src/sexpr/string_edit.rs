//! Edits whose subject is a string literal rather than a list.
//!
//! A string is the one place in a Lisp document where the parser stops
//! reporting structure: everything between the quotes is one atom, and the
//! backslashes inside it are the only thing keeping the closing quote from
//! arriving early. Every operation here is therefore written against the
//! escape run, not against the raw bytes — an offset that looks like it sits
//! on an ordinary character can be the second half of `\"`.
//!
//! [`Edit::escape_string`] and [`Edit::unescape_string`] are exact inverses on
//! purpose. Unescaping refuses any sequence other than `\\` and `\"` rather
//! than collapsing it, because `"a\nb"` means *a newline* in Emacs Lisp and
//! *the letter n* in Common Lisp, and a command that guessed would silently
//! rewrite one dialect's data as the other's.

use super::edit::{Edit, is_string_literal, replace_span, validate_edit_context};
use super::error::{SexprResult, StructureError};
use super::navigation::is_string_atom;
use super::reader_prefix_edit::content_span;
use super::tree::{Selection, SyntaxTree};
use super::types::ByteSpan;

impl Edit {
    /// Wraps the selected expression in a string literal, escaping whatever it
    /// contains so the result reads back as the same text.
    ///
    /// This is `paredit-meta-doublequote`: `(a "b")` becomes `"(a \"b\")"`. The
    /// escaping is what makes it more than a `--delimiter` value — a string is
    /// not a delimiter pair, it is a quotation.
    pub fn wrap_string(
        input: &str,
        tree: &SyntaxTree,
        selection: Selection<'_>,
    ) -> SexprResult<String> {
        validate_edit_context(input, tree, selection)?;
        Ok(replace_span(
            input,
            selection.span(),
            &format!("\"{}\"", escape(selection.text())),
        ))
    }

    /// Escapes the selected string literal's contents one level, so the result
    /// can be embedded inside another string.
    ///
    /// `"a\"b"` becomes `"a\\\"b"`.
    pub fn escape_string(
        input: &str,
        tree: &SyntaxTree,
        selection: Selection<'_>,
    ) -> SexprResult<String> {
        let (span, contents) = string_contents(input, tree, selection)?;
        Ok(replace_span(
            input,
            span,
            &format!("\"{}\"", escape(contents)),
        ))
    }

    /// Reverses one level of escaping in the selected string literal.
    ///
    /// The exact inverse of [`Edit::escape_string`]: it collapses `\\` and `\"`
    /// and refuses every other backslash sequence rather than deciding what the
    /// dialect meant by it.
    pub fn unescape_string(
        input: &str,
        tree: &SyntaxTree,
        selection: Selection<'_>,
    ) -> SexprResult<String> {
        let (span, contents) = string_contents(input, tree, selection)?;
        Ok(replace_span(
            input,
            span,
            &format!("\"{}\"", unescape(contents)?),
        ))
    }

    /// Splits the string literal containing `offset` into two adjacent strings.
    ///
    /// `"foobar"` split before `b` yields `"foo" "bar"`. The counterpart of
    /// `edit join`, which already merges two adjacent string literals into one.
    pub fn split_string(input: &str, tree: &SyntaxTree, offset: usize) -> SexprResult<String> {
        if offset > input.len() {
            return Err(StructureError::OffsetOutsideDocument {
                offset,
                length: input.len(),
            }
            .into());
        }

        let span = enclosing_string_span(tree, offset)
            .ok_or(StructureError::NotInsideStringLiteral { offset })?;
        let open = span.start().get();
        let close = span.end().get().saturating_sub(1);
        if offset <= open || offset >= close {
            return Err(StructureError::SplitStringAtDelimiter.into());
        }
        if in_escape_sequence(input, open, offset) {
            return Err(StructureError::SplitStringInEscape.into());
        }

        let mut output = String::with_capacity(input.len() + 3);
        output.push_str(&input[..offset]);
        output.push_str("\" \"");
        output.push_str(&input[offset..]);
        Ok(output)
    }
}

/// The selected string literal's span and its contents between the quotes.
fn string_contents<'a>(
    input: &'a str,
    tree: &SyntaxTree,
    selection: Selection<'_>,
) -> SexprResult<(ByteSpan, &'a str)> {
    validate_edit_context(input, tree, selection)?;
    let node = selection.node();
    if !node.reader_prefix_spans().is_empty() {
        return Err(StructureError::StringReaderPrefix.into());
    }
    let span = content_span(node);
    let text = span.slice(input);
    if !is_string_atom(node, input) || !is_string_literal(text) {
        return Err(StructureError::NotAStringLiteral.into());
    }
    Ok((span, &text[1..text.len() - 1]))
}

/// The span of the string literal containing `offset`, delimiters included.
fn enclosing_string_span(tree: &SyntaxTree, offset: usize) -> Option<ByteSpan> {
    let node_id = tree.innermost_node_at(offset)?;
    let node = tree.node(node_id);
    is_string_atom(node, tree.source()).then(|| content_span(node))
}

/// Whether `offset` falls immediately after an odd run of backslashes, i.e. in
/// the middle of an escape sequence rather than between two characters.
fn in_escape_sequence(input: &str, string_start: usize, offset: usize) -> bool {
    let mut backslashes = 0usize;
    let mut cursor = offset;
    while cursor > string_start && input.as_bytes()[cursor - 1] == b'\\' {
        backslashes += 1;
        cursor -= 1;
    }
    backslashes % 2 == 1
}

/// Escapes the two characters a Lisp string literal cannot carry raw.
fn escape(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    for character in text.chars() {
        if matches!(character, '\\' | '"') {
            output.push('\\');
        }
        output.push(character);
    }
    output
}

/// Collapses `\\` and `\"`, refusing every other backslash sequence.
fn unescape(text: &str) -> SexprResult<String> {
    let mut output = String::with_capacity(text.len());
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            output.push(character);
            continue;
        }
        match characters.next() {
            Some(escaped @ ('\\' | '"')) => output.push(escaped),
            Some(other) => {
                return Err(
                    StructureError::UnescapeUnsupportedSequence { character: other }.into(),
                );
            }
            None => return Err(StructureError::UnescapeDanglingBackslash.into()),
        }
    }
    Ok(output)
}

/// Exposed for the cursor edits, which need the same escape-run reasoning to
/// decide whether a byte is a `\` or the character one escapes.
pub(in crate::sexpr) fn escapes_next_character(
    input: &str,
    string_start: usize,
    offset: usize,
) -> bool {
    input.as_bytes().get(offset) == Some(&b'\\') && !in_escape_sequence(input, string_start, offset)
}

/// Exposed for the cursor edits: whether the byte at `offset` is the escaped
/// half of a `\x` pair.
pub(in crate::sexpr) fn is_escaped_character(
    input: &str,
    string_start: usize,
    offset: usize,
) -> bool {
    in_escape_sequence(input, string_start, offset)
}

/// Exposed for the cursor edits: the string literal containing `offset`.
pub(in crate::sexpr) fn string_span_at(tree: &SyntaxTree, offset: usize) -> Option<ByteSpan> {
    enclosing_string_span(tree, offset)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::sexpr::ExpressionPath;

    fn at_path(
        source: &str,
        path: &str,
        edit: impl Fn(&str, &SyntaxTree, Selection<'_>) -> SexprResult<String>,
    ) -> SexprResult<String> {
        let tree = SyntaxTree::parse(source).unwrap();
        let selection = tree
            .select_path(&path.parse::<ExpressionPath>().unwrap())
            .unwrap();
        edit(source, &tree, selection)
    }

    #[test]
    fn wrap_string_quotes_and_escapes_the_selection() {
        let output = at_path("(list (a \"b\"))", "0.1", Edit::wrap_string).unwrap();
        assert_eq!(output, "(list \"(a \\\"b\\\")\")");
    }

    #[test]
    fn wrap_string_wraps_a_bare_symbol() {
        let output = at_path("(list x)", "0.1", Edit::wrap_string).unwrap();
        assert_eq!(output, "(list \"x\")");
    }

    #[test]
    fn escape_and_unescape_are_inverses() {
        let source = "(list \"a\\\"b\")";
        let escaped = at_path(source, "0.1", Edit::escape_string).unwrap();
        assert_eq!(escaped, "(list \"a\\\\\\\"b\")");
        let restored = at_path(&escaped, "0.1", Edit::unescape_string).unwrap();
        assert_eq!(restored, source);
    }

    #[test]
    fn unescape_refuses_a_sequence_it_did_not_produce() {
        let error = at_path("(list \"a\\nb\")", "0.1", Edit::unescape_string).unwrap_err();
        assert!(
            error.to_string().contains("unescape only reverses"),
            "{error}"
        );
    }

    #[test]
    fn escape_refuses_a_form_that_is_not_a_string() {
        let error = at_path("(list x)", "0.1", Edit::escape_string).unwrap_err();
        assert!(error.to_string().contains("string literal"), "{error}");
    }

    fn split(source: &str, at: usize) -> SexprResult<String> {
        let tree = SyntaxTree::parse(source).unwrap();
        Edit::split_string(source, &tree, at)
    }

    #[test]
    fn split_string_produces_two_adjacent_literals() {
        let source = "(list \"foobar\")";
        let output = split(source, source.find("bar").unwrap()).unwrap();
        assert_eq!(output, "(list \"foo\" \"bar\")");
        assert!(SyntaxTree::parse(&output).is_ok());
    }

    #[test]
    fn split_string_refuses_the_delimiters() {
        let source = "(list \"foobar\")";
        assert!(split(source, source.find('"').unwrap()).is_err());
        assert!(split(source, source.rfind('"').unwrap()).is_err());
    }

    #[test]
    fn split_string_refuses_the_middle_of_an_escape() {
        let source = "(list \"a\\\"b\")";
        let escape_at = source.find('\\').unwrap();
        assert!(split(source, escape_at + 1).is_err());
    }

    #[test]
    fn split_string_refuses_an_offset_outside_any_string() {
        let source = "(list foobar)";
        assert!(split(source, source.find("bar").unwrap()).is_err());
    }

    #[test]
    fn split_then_join_returns_the_original_string() {
        let source = "(list \"foobar\")";
        let split_output = split(source, source.find("bar").unwrap()).unwrap();
        let tree = SyntaxTree::parse(&split_output).unwrap();
        let selection = tree
            .select_path(&"0.1".parse::<ExpressionPath>().unwrap())
            .unwrap();
        let joined = Edit::join(&split_output, &tree, selection).unwrap();
        assert_eq!(joined, source);
    }
}
