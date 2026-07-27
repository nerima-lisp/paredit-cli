use anyhow::Result;

use crate::domain::sexpr::{ByteOffset, ByteSpan, ExpressionKind, ExpressionView};

use super::FunctionParameterInsert;

pub(super) type SpanEdit = (ByteSpan, String);

pub(super) fn insertion_edit_for_list_item(
    container: &ExpressionView,
    protected_prefix_count: usize,
    value: &str,
    insert: FunctionParameterInsert,
) -> Result<SpanEdit> {
    if container.kind != ExpressionKind::List || container.delimiter.is_none() {
        anyhow::bail!("add-function-parameter insertion target must be a list");
    }
    if container.span.len() < 2 {
        anyhow::bail!("add-function-parameter insertion target has an invalid span");
    }

    let item_start = protected_prefix_count.min(container.children.len());
    let items = &container.children[item_start..];
    let (offset, replacement) = match insert {
        FunctionParameterInsert::Start => match items.first() {
            Some(first) => (first.span.start().get(), format!("{value} ")),
            None => {
                let close = container.span.end().get() - 1;
                let prefix = if container.children.is_empty() {
                    ""
                } else {
                    " "
                };
                (close, format!("{prefix}{value}"))
            }
        },
        FunctionParameterInsert::End => {
            let close = container.span.end().get() - 1;
            let prefix = if container.children.is_empty() {
                ""
            } else {
                " "
            };
            (close, format!("{prefix}{value}"))
        }
    };

    Ok((
        ByteSpan::new(ByteOffset::new(offset), ByteOffset::new(offset)),
        replacement,
    ))
}

pub(super) fn removal_edit_for_list_item(
    input: &str,
    container: &ExpressionView,
    item_index: usize,
) -> Result<SpanEdit> {
    if container.kind != ExpressionKind::List || container.delimiter.is_none() {
        anyhow::bail!("remove-function-parameter removal target must be a list");
    }
    if item_index >= container.children.len() {
        anyhow::bail!("remove-function-parameter removal item index {item_index} is out of bounds");
    }

    let item = &container.children[item_index];
    let span = if container.children.len() == 1 {
        item.span
    } else if item_index > 0 && is_dotted_list_separator(&container.children[item_index - 1]) {
        let Some(previous) = item_index
            .checked_sub(2)
            .and_then(|index| container.children.get(index))
        else {
            anyhow::bail!("remove-function-parameter dotted tail must follow a parameter binding");
        };
        ByteSpan::new(previous.span.end(), item.span.end())
    } else if item_index == 0 {
        let next = &container.children[1];
        // Only consume up to the newline that ends this item's own line: the
        // gap beyond that newline is the next item's leading trivia (for
        // example a comment describing it), and removing the first item must
        // not take that with it.
        let end = first_newline_or(input, item.span.end().get(), next.span.start().get());
        ByteSpan::new(item.span.start(), ByteOffset::new(end))
    } else {
        let previous = &container.children[item_index - 1];
        ByteSpan::new(previous.span.end(), item.span.end())
    };

    Ok((span, String::new()))
}

/// Returns the byte offset of the first newline in `input[start..end]`, or
/// `end` if the gap has no newline (the two items share a line).
fn first_newline_or(input: &str, start: usize, end: usize) -> usize {
    input.as_bytes()[start..end]
        .iter()
        .position(|&byte| byte == b'\n')
        .map_or(end, |offset| start + offset)
}

pub(super) fn apply_byte_span_edits(input: &str, mut edits: Vec<SpanEdit>) -> Result<String> {
    edits.sort_by_key(|(span, _)| span.start());
    ensure_non_overlapping_spans(edits.iter().map(|(span, _)| *span))?;

    let mut output = input.to_owned();
    for (span, replacement) in edits.into_iter().rev() {
        output.replace_range(span.as_range(), &replacement);
    }
    Ok(output)
}

pub(super) fn ensure_non_overlapping_spans(
    spans: impl IntoIterator<Item = ByteSpan>,
) -> Result<()> {
    let mut previous_end = None;
    for span in spans {
        let start = span.start().get();
        let end = span.end().get();
        if let Some(previous_end) = previous_end {
            if start < previous_end {
                anyhow::bail!("refusing overlapping rewrite spans");
            }
        }
        previous_end = Some(end);
    }
    Ok(())
}

pub(super) const fn spans_overlap(left: ByteSpan, right: ByteSpan) -> bool {
    left.start().get() < right.end().get() && right.start().get() < left.end().get()
}

pub(super) fn atom_child(view: &ExpressionView, index: usize) -> Option<&str> {
    view.children.get(index).and_then(atom_text)
}

pub(super) fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}

pub(super) fn list_head(view: &ExpressionView) -> Option<&str> {
    if view.kind != ExpressionKind::List {
        return None;
    }
    atom_child(view, 0)
}

pub(super) fn is_dotted_list_separator(child: &ExpressionView) -> bool {
    atom_text(child).is_some_and(|text| text == ".")
}
