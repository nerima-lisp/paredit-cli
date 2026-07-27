use anyhow::Result;

use crate::domain::sexpr::ByteSpan;

type SpanEdit = (ByteSpan, String);

pub(super) fn apply_nested_span_edits(
    outer_text: &str,
    outer_span: ByteSpan,
    mut edits: Vec<SpanEdit>,
) -> Result<String> {
    edits.sort_by_key(|(span, _)| span.start());
    ensure_non_overlapping_spans(edits.iter().map(|(span, _)| *span))?;

    let outer_start = outer_span.start().get();
    let mut output = outer_text.to_owned();
    for (span, replacement) in edits.into_iter().rev() {
        let start = span.start().get() - outer_start;
        let end = span.end().get() - outer_start;
        output.replace_range(start..end, &replacement);
    }
    Ok(output)
}

fn ensure_non_overlapping_spans(spans: impl IntoIterator<Item = ByteSpan>) -> Result<()> {
    let mut previous: Option<ByteSpan> = None;
    for span in spans {
        if let Some(previous) = previous {
            if previous.end() > span.start() {
                anyhow::bail!("overlapping replacement spans are not supported");
            }
        }
        previous = Some(span);
    }
    Ok(())
}

pub(super) fn replace_span(input: &str, span: ByteSpan, replacement: &str) -> String {
    let mut output = String::with_capacity(input.len() - span.len() + replacement.len());
    output.push_str(&input[..span.start().get()]);
    output.push_str(replacement);
    output.push_str(&input[span.end().get()..]);
    output
}
