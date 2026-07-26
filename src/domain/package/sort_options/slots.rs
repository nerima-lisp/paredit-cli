use anyhow::Result;

use crate::domain::sexpr::{ByteOffset, ByteSpan, ExpressionKind, ExpressionView, Path};

use super::{OptionSlot, PackageOptionSortOrder, ordering};
use crate::domain::leading_trivia::first_newline_or;
use crate::domain::package::syntax::{atom_text, package_option_name};

pub(super) fn collect_option_slots(
    input: &str,
    view: &ExpressionView,
    defpackage_path: &Path,
    order: PackageOptionSortOrder,
) -> Result<Vec<OptionSlot>> {
    let options = view.children.iter().skip(2).collect::<Vec<_>>();
    let head_end = view.children[1].span.end().get();
    let slot_spans = build_option_slot_spans(input, head_end, &options);

    options
        .iter()
        .zip(slot_spans)
        .enumerate()
        .map(|(option_index, (option, slot_span))| {
            let has_leading_trivia = slot_span.start() != option.span.start();
            analyze_option_slot(
                input,
                option,
                slot_span,
                has_leading_trivia,
                defpackage_path,
                option_index,
                order,
            )
        })
        .collect()
}

/// Splits the option list into one contiguous, gap-free slot per option.
/// Each slot begins at the newline that ends the previous option's line (or
/// right after the package name for the first option), so a leading `;;`
/// comment moves with the option below it instead of staying fixed in place.
fn build_option_slot_spans(
    input: &str,
    head_end: usize,
    options: &[&ExpressionView],
) -> Vec<ByteSpan> {
    let mut starts = Vec::with_capacity(options.len());
    for (index, option) in options.iter().enumerate() {
        let previous_end = if index == 0 {
            head_end
        } else {
            options[index - 1].span.end().get()
        };
        let this_start = option.span.start().get();
        starts.push(first_newline_or(input, previous_end, this_start));
    }

    starts
        .iter()
        .enumerate()
        .map(|(index, &start)| {
            let end = match starts.get(index + 1) {
                Some(&next_start) => next_start,
                None => options[index].span.end().get(),
            };
            ByteSpan::new(ByteOffset::new(start), ByteOffset::new(end))
        })
        .collect()
}

fn analyze_option_slot(
    input: &str,
    option: &ExpressionView,
    slot_span: ByteSpan,
    has_leading_trivia: bool,
    defpackage_path: &Path,
    option_index: usize,
    order: PackageOptionSortOrder,
) -> Result<OptionSlot> {
    if option.kind != ExpressionKind::List || option.children.is_empty() {
        anyhow::bail!(
            "cannot sort defpackage options at {defpackage_path}; only direct option lists are supported"
        );
    }
    let Some(option_head) = atom_text(&option.children[0]) else {
        anyhow::bail!(
            "cannot sort defpackage option at {}; option head must be an atom",
            defpackage_path.child(option_index)
        );
    };

    let name = package_option_name(option_head);
    let payload = option
        .children
        .iter()
        .skip(1)
        .find_map(atom_text)
        .unwrap_or("")
        .to_owned();
    let label = if payload.is_empty() {
        option_head.to_owned()
    } else {
        format!("{option_head} {payload}")
    };
    let bare_text = option.span.slice(input);
    let sort_key = ordering::option_sort_key(&name, &payload, bare_text, order);

    Ok(OptionSlot {
        full_span: slot_span,
        full_text: slot_span.slice(input).to_owned(),
        has_leading_trivia,
        label,
        sort_key,
    })
}
