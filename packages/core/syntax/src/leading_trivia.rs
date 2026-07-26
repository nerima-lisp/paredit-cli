//! Shared helpers for keeping a sibling's leading trivia (a `;;` comment or a
//! blank run) attached to it when use cases reorder, move, or relocate
//! sibling forms. Comments live outside the syntax tree, so any rewrite that
//! rebuilds text from spans has to track this trivia explicitly or it is
//! silently dropped.

/// Returns the byte offset of the first newline in `input[start..end]`, or
/// `start` if the gap has no newline (the two forms share a line, so there
/// is no line boundary to anchor a trivia split on and the following
/// sibling's slot begins immediately after the previous one, capturing the
/// whole gap). Callers use this to find where a sibling's own leading trivia
/// begins: the newline that ends the *previous* sibling's line, so anything
/// after it (blank lines, then an own-line comment) belongs to the sibling
/// that follows.
pub(crate) fn first_newline_or(input: &str, start: usize, end: usize) -> usize {
    input.as_bytes()[start..end]
        .iter()
        .position(|&byte| byte == b'\n')
        .map_or(start, |offset| start + offset)
}

/// Collapses a leading run of genuinely blank lines (two or more newlines in
/// a row) down to the single separator newline a relocated sibling still
/// needs, leaving any leading comment and its own indentation untouched.
///
/// The very first newline in a sibling's leading trivia always plays double
/// duty: it ends the previous line *and* starts this sibling's own line. When
/// there is no blank line at all (just `"\n"` before the sibling), that
/// single newline must be kept — dropping it unconditionally would glue a
/// relocated sibling onto whatever text now precedes it.
pub(crate) fn strip_leading_blank_lines(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut cursor = 0;
    while bytes.get(cursor) == Some(&b'\n') && bytes.get(cursor + 1) == Some(&b'\n') {
        cursor += 1;
    }
    text[cursor..].to_owned()
}
