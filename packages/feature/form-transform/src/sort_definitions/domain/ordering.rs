use std::cmp::Ordering;

use super::types::{DefinitionEntry, SortDefinitionsStrategy};

pub(super) fn sorted_entry_positions(
    entries: &[DefinitionEntry],
    strategy: SortDefinitionsStrategy,
) -> Vec<usize> {
    let mut positions = (0..entries.len()).collect::<Vec<_>>();
    positions.sort_by(|left, right| compare_entries(&entries[*left], &entries[*right], strategy));
    positions
}

fn compare_entries(
    left: &DefinitionEntry,
    right: &DefinitionEntry,
    strategy: SortDefinitionsStrategy,
) -> Ordering {
    match strategy {
        SortDefinitionsStrategy::Name => compare_by_name(left, right),
        SortDefinitionsStrategy::KindThenName => left
            .item
            .category
            .label()
            .cmp(right.item.category.label())
            .then_with(|| compare_by_name(left, right)),
    }
}

fn compare_by_name(left: &DefinitionEntry, right: &DefinitionEntry) -> Ordering {
    left.item
        .name
        .is_none()
        .cmp(&right.item.name.is_none())
        .then_with(|| {
            left.item
                .name
                .as_deref()
                .unwrap_or("")
                .cmp(right.item.name.as_deref().unwrap_or(""))
        })
        .then_with(|| left.item.category.label().cmp(right.item.category.label()))
        .then_with(|| left.item.head.cmp(&right.item.head))
        .then_with(|| left.item.source_index.cmp(&right.item.source_index))
}
