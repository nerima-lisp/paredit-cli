use crate::error::{FormTransformResult, TransformSelectorError};

use paredit_core_syntax::sexpr::ByteSpan;

pub fn ensure_non_overlapping_spans(
    spans: impl IntoIterator<Item = ByteSpan>,
) -> FormTransformResult<()> {
    let mut previous_end = None;
    for span in spans {
        let start = span.start().get();
        let end = span.end().get();
        if let Some(previous_end) = previous_end {
            if start < previous_end {
                return Err(TransformSelectorError::OverlappingRewriteSpans.into());
            }
        }
        previous_end = Some(end);
    }
    Ok(())
}

pub fn append_top_level_definitions(input: &str, definitions: &[String]) -> String {
    let mut output = input.trim_end().to_owned();
    for definition in definitions {
        if !output.is_empty() {
            output.push_str("\n\n");
        }
        output.push_str(definition);
    }
    if !definitions.is_empty() {
        output.push('\n');
    }
    output
}

pub fn replace_byte_span(input: &str, span: ByteSpan, replacement: &str) -> String {
    let mut output = String::with_capacity(input.len() + replacement.len());
    output.push_str(&input[..span.start().get()]);
    output.push_str(replacement);
    output.push_str(&input[span.end().get()..]);
    output
}
