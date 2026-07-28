use crate::error::{InlineInternalError, InlineResult};

use paredit_core_syntax::sexpr::{ByteOffset, ByteSpan};

pub fn apply_relative_body_edits(
    input: &str,
    body_span: ByteSpan,
    mut replacements: Vec<(ByteSpan, String)>,
) -> InlineResult<String> {
    replacements.sort_by_key(|(span, _)| span.start());
    ensure_non_overlapping_spans(replacements.iter().map(|(span, _)| *span))?;

    let body_start = body_span.start().get();
    let mut output = body_span.slice(input).to_owned();
    for (span, replacement) in replacements.into_iter().rev() {
        let start = span.start().get() - body_start;
        let end = span.end().get() - body_start;
        output.replace_range(start..end, &replacement);
    }
    Ok(output)
}

pub fn apply_byte_span_edits(
    input: &str,
    mut edits: Vec<(ByteSpan, String)>,
) -> InlineResult<String> {
    edits.sort_by_key(|(span, _)| span.start());
    ensure_non_overlapping_spans(edits.iter().map(|(span, _)| *span))?;

    let mut output = input.to_owned();
    for (span, replacement) in edits.into_iter().rev() {
        output.replace_range(span.as_range(), &replacement);
    }
    Ok(output)
}

pub fn ensure_non_overlapping_spans(spans: impl IntoIterator<Item = ByteSpan>) -> InlineResult<()> {
    let mut previous_end = None;
    for span in spans {
        let start = span.start().get();
        let end = span.end().get();
        if let Some(previous_end) = previous_end {
            if start < previous_end {
                return Err(InlineInternalError::OverlappingRewriteSpans.into());
            }
        }
        previous_end = Some(end);
    }
    Ok(())
}

pub fn expand_definition_removal(input: &str, span: ByteSpan) -> ByteSpan {
    let bytes = input.as_bytes();
    let mut start = span.start().get();
    let mut end = span.end().get();
    if end < bytes.len() && bytes[end].is_ascii_whitespace() {
        while end < bytes.len() && bytes[end].is_ascii_whitespace() {
            end += 1;
        }
    } else {
        while start > 0 && bytes[start - 1].is_ascii_whitespace() {
            start -= 1;
        }
    }
    ByteSpan::new(ByteOffset::new(start), ByteOffset::new(end))
}
