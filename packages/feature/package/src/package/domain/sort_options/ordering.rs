use crate::domain::leading_trivia::strip_leading_blank_lines;
use crate::domain::sexpr::ByteSpan;

use super::{OptionReplacement, OptionSlot, PackageOptionSortOrder};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::domain::package) struct OptionSortKey {
    rank: usize,
    name: String,
    payload: String,
    text: String,
}

/// Separator used when the option list's original first slot (which has no
/// leading trivia of its own) is reordered away from the front.
const DEFAULT_OPTION_SEPARATOR: &str = "\n  ";

pub(super) fn sort_slots(slots: &[OptionSlot]) -> (Vec<String>, Vec<OptionReplacement>) {
    let mut order = (0..slots.len()).collect::<Vec<_>>();
    order.sort_by(|&left, &right| {
        slots[left]
            .sort_key
            .cmp(&slots[right].sort_key)
            .then_with(|| slots[left].full_text.cmp(&slots[right].full_text))
    });

    let new_options = order
        .iter()
        .map(|&index| slots[index].label.clone())
        .collect::<Vec<_>>();

    let replacements = if is_identity(&order) {
        Vec::new()
    } else {
        let region = ByteSpan::new(
            slots[0].full_span.start(),
            slots[slots.len() - 1].full_span.end(),
        );

        let mut replacement = String::new();
        for (target_offset, &source_index) in order.iter().enumerate() {
            let slot = &slots[source_index];
            // The option list's original first slot carries no leading
            // trivia of its own. If it lands anywhere but the front after
            // reordering, it needs a separator so it doesn't glue onto the
            // previous option's closing delimiter.
            if target_offset > 0 && !slot.has_leading_trivia {
                replacement.push_str(DEFAULT_OPTION_SEPARATOR);
            }
            replacement.push_str(&slot.full_text);
        }
        // When some other slot displaces the option list's original first
        // slot from the front, that slot's own leading trivia (a blank run,
        // possibly with a comment) ends up at the top of the region, where
        // there is nothing left for it to separate from. Collapsing a
        // genuinely blank run down to one newline is a no-op when the
        // original first slot stayed in front, since it never carries more
        // than a single leading newline of its own.
        let replacement = strip_leading_blank_lines(&replacement);

        vec![OptionReplacement {
            span: region,
            replacement,
        }]
    };

    (new_options, replacements)
}

fn is_identity(order: &[usize]) -> bool {
    order
        .iter()
        .enumerate()
        .all(|(index, &value)| index == value)
}

pub(super) fn option_sort_key(
    name: &str,
    payload: &str,
    text: &str,
    order: PackageOptionSortOrder,
) -> OptionSortKey {
    OptionSortKey {
        rank: option_rank(name, order),
        name: name.to_owned(),
        payload: normalized_atom(payload),
        text: text.to_ascii_lowercase(),
    }
}

fn option_rank(name: &str, order: PackageOptionSortOrder) -> usize {
    if order == PackageOptionSortOrder::Name {
        return 0;
    }

    match name {
        "nicknames" | "documentation" => 0,
        "use" => 10,
        "shadow" => 20,
        "shadowing-import-from" => 30,
        "import-from" => 40,
        "local-nicknames" => 50,
        "export" => 60,
        "intern" => 70,
        _ => 100,
    }
}

fn normalized_atom(value: &str) -> String {
    value
        .strip_prefix("#:")
        .or_else(|| value.strip_prefix(':'))
        .unwrap_or(value)
        .to_ascii_lowercase()
}
