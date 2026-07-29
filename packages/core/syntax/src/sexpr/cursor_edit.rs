//! Character-level edits that refuse to unbalance the document.
//!
//! These are the `paredit.el` commands that look like plain typing —
//! `paredit-forward-delete`, `paredit-backward-delete`, `paredit-newline` —
//! and are not. In Emacs the guard is that point *moves over* a delimiter
//! instead of deleting it. Through a CLI there is no point to move, so the
//! same guard has to become a refusal with a reason, which is what every arm
//! below produces.
//!
//! Three rules do the work:
//!
//! - A delimiter is deletable only when its pair encloses nothing. `()` and
//!   `""` vanish as a unit; `(a)` refuses.
//! - Whitespace between two symbols is not deletable, because removing it
//!   fuses two atoms into one. `edit join` already refuses the same thing for
//!   the same reason.
//! - A backslash inside a string travels with the character it escapes, in
//!   both directions, so a delete can never strand half an escape.

use super::edit::Edit;
use super::error::{SexprResult, StructureError};
use super::navigation::ContextKind;
use super::reader_prefix_edit::content_start;
use super::string_edit::{escapes_next_character, is_escaped_character, string_span_at};
use super::tree::SyntaxTree;
use super::types::{ByteOffset, ByteSpan, is_symbol_boundary};

impl Edit {
    /// Deletes the character at `offset`, refusing anything that would change
    /// the document's structure.
    ///
    /// An empty delimiter pair is deleted whole, so `()` and `""` disappear in
    /// one step rather than leaving an unmatched half behind.
    pub fn delete_forward(input: &str, tree: &SyntaxTree, offset: usize) -> SexprResult<String> {
        if offset >= input.len() {
            return Err(StructureError::NothingToDelete { offset }.into());
        }
        delete_character_at(input, tree, offset)
    }

    /// Deletes the character *before* `offset`, under the same rules as
    /// [`Edit::delete_forward`].
    pub fn delete_backward(input: &str, tree: &SyntaxTree, offset: usize) -> SexprResult<String> {
        if offset > input.len() {
            return Err(StructureError::OffsetOutsideDocument {
                offset,
                length: input.len(),
            }
            .into());
        }
        let start = previous_char_boundary(input, offset)
            .ok_or(StructureError::NothingToDelete { offset })?;
        delete_character_at(input, tree, start)
    }

    /// Inserts a newline at `offset`, refusing any position where the break
    /// would land inside text the reader treats as one unit.
    ///
    /// The caller reindents afterwards: the tree this was planned against no
    /// longer describes the result, and re-deriving indentation needs the new
    /// one. [`SyntaxTree::reindent_form_at`] is the other half.
    pub fn insert_newline(input: &str, tree: &SyntaxTree, offset: usize) -> SexprResult<String> {
        if offset > input.len() {
            return Err(StructureError::OffsetOutsideDocument {
                offset,
                length: input.len(),
            }
            .into());
        }

        let context = tree.context_at(offset)?;
        match context.kind {
            ContextKind::String => {
                return Err(StructureError::NewlineInsideOpaqueText {
                    context: "a string literal",
                }
                .into());
            }
            ContextKind::Comment => {
                return Err(StructureError::NewlineInsideOpaqueText {
                    context: "a comment",
                }
                .into());
            }
            // Inside an atom or its reader prefix a break splits one token in
            // two — except at the two boundaries where it lands *between*
            // things instead: before the whole form, and between a reader
            // prefix and the form it marks (`'` newline `foo` reads the same).
            ContextKind::Code | ContextKind::ReaderPrefix => {
                let node_id = tree
                    .innermost_node_at(offset)
                    .ok_or(StructureError::NothingToDelete { offset })?;
                let node = tree.node(node_id);
                if node.span.start().get() != offset && content_start(node).get() != offset {
                    return Err(StructureError::NewlineInsideOpaqueText {
                        context: if context.kind == ContextKind::ReaderPrefix {
                            "a reader prefix"
                        } else {
                            "a symbol"
                        },
                    }
                    .into());
                }
            }
            ContextKind::Whitespace | ContextKind::Delimiter => {}
        }

        let mut output = String::with_capacity(input.len() + 1);
        output.push_str(&input[..offset]);
        output.push('\n');
        output.push_str(&input[offset..]);
        Ok(output)
    }
}

/// Removes the character starting at `start`, or the whole unit it belongs to.
fn delete_character_at(input: &str, tree: &SyntaxTree, start: usize) -> SexprResult<String> {
    let end = next_char_boundary(input, start)
        .ok_or(StructureError::NothingToDelete { offset: start })?;
    let byte = input.as_bytes()[start];
    let context = tree.context_at(start)?;

    let removal = match context.kind {
        ContextKind::Delimiter => delimiter_removal(tree, start, byte)?,
        ContextKind::String => string_removal(input, tree, start, end)?,
        ContextKind::Comment => {
            comment_removal(tree, start)?;
            span(start, end)
        }
        ContextKind::Whitespace => {
            if fuses_symbols(input, start, end) {
                return Err(StructureError::DeleteWouldFuseSymbols.into());
            }
            span(start, end)
        }
        ContextKind::Code | ContextKind::ReaderPrefix => span(start, end),
    };

    let mut output = String::with_capacity(input.len());
    output.push_str(&input[..removal.start().get()]);
    output.push_str(&input[removal.end().get()..]);
    Ok(output)
}

/// A list delimiter is deletable only when its pair encloses nothing.
fn delimiter_removal(tree: &SyntaxTree, offset: usize, byte: u8) -> SexprResult<ByteSpan> {
    let node_id = tree
        .innermost_node_at(offset)
        .ok_or(StructureError::NothingToDelete { offset })?;
    let node = tree.node(node_id);
    let (Some(open), Some(close)) = (node.open, node.close) else {
        return Err(StructureError::DeleteWouldUnbalance {
            delimiter: byte as char,
        }
        .into());
    };
    let empty = node.children.is_empty()
        && tree.source()[open.get() + 1..close.get()]
            .chars()
            .all(char::is_whitespace);
    if !empty {
        return Err(StructureError::DeleteWouldUnbalance {
            delimiter: byte as char,
        }
        .into());
    }
    Ok(span(open.get(), close.get() + 1))
}

/// Inside a string, a backslash and the character it escapes are one unit, and
/// the quotes themselves are deletable only when the string is empty.
fn string_removal(
    input: &str,
    tree: &SyntaxTree,
    start: usize,
    end: usize,
) -> SexprResult<ByteSpan> {
    let literal = string_span_at(tree, start)
        .ok_or(StructureError::NotInsideStringLiteral { offset: start })?;
    let open = literal.start().get();
    let close = literal.end().get() - 1;

    if start == open || start == close {
        if close == open + 1 {
            return Ok(span(open, close + 1));
        }
        return Err(StructureError::DeleteWouldUnbalance { delimiter: '"' }.into());
    }

    if escapes_next_character(input, open, start) {
        let escaped_end = next_char_boundary(input, end).unwrap_or(end);
        return Ok(span(start, escaped_end));
    }
    if is_escaped_character(input, open, start) {
        let backslash = previous_char_boundary(input, start).unwrap_or(start);
        return Ok(span(backslash, end));
    }
    Ok(span(start, end))
}

/// A comment's opening token is not deletable: removing it turns the rest of
/// the comment back into code, which is the opposite of a structure-safe edit.
fn comment_removal(tree: &SyntaxTree, offset: usize) -> SexprResult<()> {
    for comment in tree.comments() {
        let start = comment.span().start().get();
        let end = comment.span().end().get();
        if offset < start || offset >= end {
            continue;
        }
        let opener = if comment.text().starts_with(';') {
            1
        } else {
            2
        };
        if offset < start + opener {
            return Err(StructureError::DeleteWouldUncomment.into());
        }
        // A block comment's terminator is structure too.
        if comment.text().ends_with("|#") && offset + 2 >= end {
            return Err(StructureError::DeleteWouldUncomment.into());
        }
    }
    Ok(())
}

/// Whether removing `input[start..end]` would butt two symbol characters
/// together, silently merging two atoms into one.
fn fuses_symbols(input: &str, start: usize, end: usize) -> bool {
    let before = input.as_bytes()[..start].last().copied();
    let after = input.as_bytes().get(end).copied();
    match (before, after) {
        (Some(before), Some(after)) => !is_symbol_boundary(before) && !is_symbol_boundary(after),
        _ => false,
    }
}

const fn span(start: usize, end: usize) -> ByteSpan {
    ByteSpan::new(ByteOffset::new(start), ByteOffset::new(end))
}

fn next_char_boundary(input: &str, offset: usize) -> Option<usize> {
    if offset >= input.len() {
        return None;
    }
    let mut end = offset + 1;
    while end < input.len() && !input.is_char_boundary(end) {
        end += 1;
    }
    Some(end)
}

fn previous_char_boundary(input: &str, offset: usize) -> Option<usize> {
    let mut start = offset.checked_sub(1)?;
    while start > 0 && !input.is_char_boundary(start) {
        start -= 1;
    }
    Some(start)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn delete_forward(source: &str, at: usize) -> SexprResult<String> {
        let tree = SyntaxTree::parse(source).unwrap();
        Edit::delete_forward(source, &tree, at)
    }

    fn delete_backward(source: &str, at: usize) -> SexprResult<String> {
        let tree = SyntaxTree::parse(source).unwrap();
        Edit::delete_backward(source, &tree, at)
    }

    #[test]
    fn deleting_a_symbol_character_is_allowed_in_both_directions() {
        let source = "(list abc)";
        assert_eq!(
            delete_forward(source, source.find('b').unwrap()).unwrap(),
            "(list ac)"
        );
        assert_eq!(
            delete_backward(source, source.find('b').unwrap()).unwrap(),
            "(list bc)"
        );
    }

    #[test]
    fn deleting_a_non_empty_list_delimiter_is_refused() {
        let source = "(list a)";
        let error = delete_forward(source, 0).unwrap_err();
        assert!(error.to_string().contains("unbalance"), "{error}");
        let error = delete_backward(source, source.len()).unwrap_err();
        assert!(error.to_string().contains("unbalance"), "{error}");
    }

    #[test]
    fn an_empty_list_is_deleted_as_a_pair_from_either_side() {
        assert_eq!(delete_forward("(list ())", 6).unwrap(), "(list )");
        assert_eq!(delete_backward("(list ())", 8).unwrap(), "(list )");
    }

    #[test]
    fn an_empty_string_is_deleted_as_a_pair() {
        assert_eq!(delete_forward("(list \"\")", 6).unwrap(), "(list )");
    }

    #[test]
    fn a_non_empty_string_delimiter_is_refused() {
        let error = delete_forward("(list \"ab\")", 6).unwrap_err();
        assert!(error.to_string().contains("unbalance"), "{error}");
    }

    #[test]
    fn an_escape_and_the_character_it_escapes_are_deleted_together() {
        let source = "(list \"a\\\"b\")";
        let backslash = source.find('\\').unwrap();
        assert_eq!(delete_forward(source, backslash).unwrap(), "(list \"ab\")");
        // Deleting the escaped quote takes the backslash with it, from the
        // other side of the pair.
        assert_eq!(
            delete_forward(source, backslash + 1).unwrap(),
            "(list \"ab\")"
        );
    }

    #[test]
    fn whitespace_between_two_symbols_is_not_deletable() {
        let source = "(list a b)";
        let error = delete_forward(source, source.find(" b").unwrap()).unwrap_err();
        assert!(
            error.to_string().contains("keeps two symbols apart"),
            "{error}"
        );
    }

    #[test]
    fn whitespace_next_to_a_delimiter_is_deletable() {
        let source = "(list a )";
        assert_eq!(
            delete_forward(source, source.find(" )").unwrap()).unwrap(),
            "(list a)"
        );
    }

    #[test]
    fn a_comment_body_is_deletable_but_its_opener_is_not() {
        let source = "(list a) ; note\n";
        let semicolon = source.find(';').unwrap();
        assert!(delete_forward(source, semicolon).is_err());
        assert_eq!(
            delete_forward(source, semicolon + 2).unwrap(),
            "(list a) ; ote\n"
        );
    }

    #[test]
    fn deleting_past_either_end_of_the_document_refuses() {
        assert!(delete_forward("(a)", 3).is_err());
        assert!(delete_backward("(a)", 0).is_err());
    }

    #[test]
    fn a_multibyte_character_is_deleted_whole() {
        let source = "(list \"\u{3042}\u{3044}\")";
        let output = delete_forward(source, source.find('\u{3042}').unwrap()).unwrap();
        assert_eq!(output, "(list \"\u{3044}\")");
    }

    fn newline(source: &str, at: usize) -> SexprResult<String> {
        let tree = SyntaxTree::parse(source).unwrap();
        Edit::insert_newline(source, &tree, at)
    }

    #[test]
    fn a_newline_between_two_forms_is_inserted() {
        let source = "(list a b)";
        assert_eq!(
            newline(source, source.find(" b").unwrap() + 1).unwrap(),
            "(list a \nb)"
        );
    }

    #[test]
    fn a_newline_inside_a_string_a_comment_or_a_symbol_is_refused() {
        let source = "(list \"ab\" abc) ; note\n";
        assert!(newline(source, source.find("b\"").unwrap()).is_err());
        assert!(newline(source, source.find("note").unwrap()).is_err());
        assert!(newline(source, source.find("bc)").unwrap()).is_err());
    }

    #[test]
    fn a_newline_in_front_of_a_symbol_is_allowed() {
        let source = "(list abc)";
        assert_eq!(
            newline(source, source.find("abc").unwrap()).unwrap(),
            "(list \nabc)"
        );
    }

    #[test]
    fn a_newline_in_front_of_a_quoted_form_is_allowed_but_not_mid_prefix() {
        // The break belongs *before* the whole form, and the reader prefix is
        // part of the form: refusing here would make a legal position
        // unreachable just because the form happens to be quoted.
        let source = "(list ,@foo)";
        let prefix = source.find(",@").unwrap();
        assert_eq!(newline(source, prefix).unwrap(), "(list \n,@foo)");
        // Between the prefix and its form is also legal syntax.
        assert_eq!(newline(source, prefix + 2).unwrap(), "(list ,@\nfoo)");
        // Halfway through `,@` is not.
        assert!(newline(source, prefix + 1).is_err());
    }
}
