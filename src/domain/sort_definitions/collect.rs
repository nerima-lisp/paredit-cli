use anyhow::Result;

use crate::domain::definition::{DefinitionCategory, definition_shape};
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{Path, SyntaxTree};

use super::super::leading_trivia::first_newline_or;
use super::syntax::list_head;
use super::types::{DefinitionBlock, DefinitionEntry, RawDefinition, SortDefinitionsItem};

pub(super) fn collect_sortable_blocks(
    input: &str,
    tree: &SyntaxTree,
    dialect: Dialect,
) -> Result<Vec<DefinitionBlock>> {
    let mut blocks = Vec::new();
    let mut current = Vec::new();

    for index in 0..tree.root_children().len() {
        let path = Path::root_child(index);
        let selection = tree.select_path(&path)?;
        let view = selection.view();
        let Some(head) = list_head(&view) else {
            finish_block(input, &mut current, &mut blocks);
            continue;
        };
        let Some(shape) = definition_shape(dialect, &view, head) else {
            finish_block(input, &mut current, &mut blocks);
            continue;
        };

        if !is_sortable_category(shape.category) {
            finish_block(input, &mut current, &mut blocks);
            continue;
        }

        let Some(name) = shape.name(&view).map(ToOwned::to_owned) else {
            finish_block(input, &mut current, &mut blocks);
            continue;
        };

        current.push(RawDefinition {
            path,
            span: selection.span(),
            head: head.to_owned(),
            name: Some(name),
            category: shape.category,
            source_index: index,
        });
    }
    finish_block(input, &mut current, &mut blocks);

    Ok(blocks)
}

fn finish_block(input: &str, current: &mut Vec<RawDefinition>, blocks: &mut Vec<DefinitionBlock>) {
    if current.len() < 2 {
        current.clear();
        return;
    }

    let slot_starts = compute_slot_starts(input, current);
    let mut entries = Vec::with_capacity(current.len());
    for index in 0..current.len() {
        let slot_start = slot_starts[index];
        let slot_end = match slot_starts.get(index + 1) {
            Some(&next_start) => next_start,
            None => current[index].span.end().get(),
        };
        entries.push(DefinitionEntry {
            item: SortDefinitionsItem {
                old_path: current[index].path.clone(),
                new_path: current[index].path.clone(),
                span: current[index].span,
                head: current[index].head.clone(),
                name: current[index].name.clone(),
                category: current[index].category,
                source_index: current[index].source_index,
                target_index: current[index].source_index,
            },
            form_text: input[slot_start..slot_end].to_owned(),
            has_leading_trivia: index != 0,
        });
    }

    let Some(first) = current.first() else {
        current.clear();
        return;
    };
    let Some(last) = current.last() else {
        current.clear();
        return;
    };

    blocks.push(DefinitionBlock {
        start: first.span.start().get(),
        end: last.span.end().get(),
        entries,
    });
    current.clear();
}

/// Computes each definition's slot start. The first entry starts at its own
/// span, since nothing in the block precedes it. Every later entry starts at
/// the newline that ends the previous entry's line (or the previous entry's
/// own end, if the two share a line), so any full-line comments and blank
/// runs between two definitions become the following definition's leading
/// trivia and move with it when the block is reordered.
fn compute_slot_starts(input: &str, current: &[RawDefinition]) -> Vec<usize> {
    let mut starts = Vec::with_capacity(current.len());
    for (index, definition) in current.iter().enumerate() {
        if index == 0 {
            starts.push(definition.span.start().get());
            continue;
        }
        let previous_end = current[index - 1].span.end().get();
        let this_start = definition.span.start().get();
        starts.push(first_newline_or(input, previous_end, this_start));
    }
    starts
}

const fn is_sortable_category(category: DefinitionCategory) -> bool {
    !matches!(
        category,
        DefinitionCategory::Package | DefinitionCategory::System
    )
}
