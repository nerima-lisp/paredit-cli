use std::collections::BTreeMap;

use super::{OptionMerge, OptionReplacement, OptionSlot};
use crate::domain::package::syntax::normalize_package_atom;
use crate::domain::sexpr::SyntaxTree;

pub(super) fn merge_slots(
    slots: &[OptionSlot],
    tree: &SyntaxTree,
) -> (Vec<OptionMerge>, Vec<OptionReplacement>) {
    let mut groups = BTreeMap::<(String, Option<String>), Vec<&OptionSlot>>::new();
    for slot in slots {
        groups
            .entry((slot.name.clone(), slot.key.clone()))
            .or_default()
            .push(slot);
    }

    let mut merges = Vec::new();
    let mut replacements = Vec::new();
    for ((_name, key), group) in groups {
        if group.len() < 2 {
            continue;
        }
        // Merging rebuilds the kept option's text from parsed atoms and
        // blanks the others; both steps only see the node tree, so a
        // comment anywhere in the group's source would be silently dropped.
        // Leave the group untouched rather than lose it.
        if group.iter().any(|slot| tree.has_comment_in(slot.span)) {
            continue;
        }
        let kept = group[0];
        let merged_atoms = merged_body_atoms(&group);
        let old_atoms = group
            .iter()
            .flat_map(|slot| slot.body_atoms.clone())
            .collect::<Vec<_>>();

        replacements.push(OptionReplacement {
            span: kept.span,
            replacement: format_option(&kept.head_text, &merged_atoms),
        });
        replacements.extend(group.iter().skip(1).map(|slot| OptionReplacement {
            span: slot.span,
            replacement: String::new(),
        }));

        merges.push(OptionMerge {
            head: kept.head_text.clone(),
            key,
            kept_path: kept.path.clone(),
            kept_span: kept.span,
            removed_paths: group.iter().skip(1).map(|slot| slot.path.clone()).collect(),
            removed_spans: group.iter().skip(1).map(|slot| slot.span).collect(),
            old_atoms,
            new_atoms: merged_atoms,
        });
    }

    (merges, replacements)
}

pub(super) fn merge_key(name: &str, body_atoms: &[String]) -> Option<Option<String>> {
    match name {
        "export" | "intern" | "nicknames" | "shadow" | "use" => Some(None),
        "import-from" | "shadowing-import-from" => {
            body_atoms.first().map(|atom| Some(normalized_atom(atom)))
        }
        _ => None,
    }
}

fn merged_body_atoms(group: &[&OptionSlot]) -> Vec<String> {
    if group[0].key.is_some() {
        let mut merged = vec![group[0].body_atoms[0].clone()];
        for slot in group {
            push_unique_atoms(&mut merged, slot.body_atoms.iter().skip(1));
        }
        return merged;
    }

    let mut merged = Vec::new();
    for slot in group {
        push_unique_atoms(&mut merged, slot.body_atoms.iter());
    }
    merged
}

fn push_unique_atoms<'a>(merged: &mut Vec<String>, atoms: impl Iterator<Item = &'a String>) {
    for atom in atoms {
        let normalized = normalized_atom(atom);
        if merged
            .iter()
            .any(|existing| normalized_atom(existing) == normalized)
        {
            continue;
        }
        merged.push(atom.clone());
    }
}

fn format_option(head: &str, atoms: &[String]) -> String {
    if atoms.is_empty() {
        return format!("({head})");
    }
    format!("({head} {})", atoms.join(" "))
}

fn normalized_atom(value: &str) -> String {
    normalize_package_atom(value).to_ascii_lowercase()
}
