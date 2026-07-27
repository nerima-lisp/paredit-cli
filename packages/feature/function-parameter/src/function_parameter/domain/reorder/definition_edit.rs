use anyhow::{Context, Result};

use crate::domain::sexpr::{ExpressionKind, ExpressionView, SyntaxTree};

use super::super::list_edit::SpanEdit;
use super::parameter::ReorderableParameter;

pub(in crate::domain::function_parameter) fn reorder_function_definition_edit(
    input: &str,
    tree: &SyntaxTree,
    container: &ExpressionView,
    parameters: &[ReorderableParameter],
    new_relative_order: &[usize],
    operation: &str,
) -> Result<SpanEdit> {
    if container.kind != ExpressionKind::List || container.delimiter.is_none() {
        anyhow::bail!("{operation} reorder target must be a list");
    }
    // The parameter list is rebuilt from each parameter's own bare span text
    // joined with a single space; a comment anywhere in the list lives
    // outside the tree and has no slot in the rebuilt text, so it would be
    // silently dropped.
    if tree.has_comment_in(container.span) {
        anyhow::bail!(
            "{operation} cannot reorder a parameter list that contains a comment, \
             which would be discarded when the list is rebuilt; remove or relocate \
             the comment first"
        );
    }

    let start = container.span.start().get();
    let end = container.span.end().get();
    let open = &input[start..start + 1];
    let close = &input[end - 1..end];
    let reordered_parameter_items = new_relative_order
        .iter()
        .map(|&index| {
            let item_index = parameters[index].item_index;
            Ok(container.children[item_index].span.slice(input).to_owned())
        })
        .collect::<Result<Vec<_>>>()?;
    let mut reordered_parameter_items = reordered_parameter_items.into_iter();

    let items = container
        .children
        .iter()
        .enumerate()
        .map(|(item_index, child)| {
            if parameters
                .iter()
                .any(|parameter| parameter.item_index == item_index)
            {
                reordered_parameter_items.next().with_context(|| {
                    format!("{operation} definition reorder produced an incomplete parameter list")
                })
            } else {
                Ok(child.span.slice(input).to_owned())
            }
        })
        .collect::<Result<Vec<_>>>()?;

    let replacement = if items.is_empty() {
        format!("{open}{close}")
    } else {
        format!("{open}{}{close}", items.join(" "))
    };
    Ok((container.span, replacement))
}
