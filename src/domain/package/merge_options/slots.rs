use anyhow::{Context, Result};

use crate::domain::sexpr::{ExpressionKind, ExpressionView, Path};

use super::OptionSlot;
use crate::domain::package::syntax::{atom_text, package_option_name};

pub(super) fn collect_option_slots(
    view: &ExpressionView,
    defpackage_path: &Path,
) -> Result<Vec<OptionSlot>> {
    view.children
        .iter()
        .enumerate()
        .skip(2)
        .map(|(option_index, option)| analyze_option_slot(option, defpackage_path, option_index))
        .collect::<Result<Vec<_>>>()
        .map(|slots| slots.into_iter().flatten().collect())
}

fn analyze_option_slot(
    option: &ExpressionView,
    defpackage_path: &Path,
    option_index: usize,
) -> Result<Option<OptionSlot>> {
    if option.kind != ExpressionKind::List || option.children.is_empty() {
        anyhow::bail!(
            "cannot merge defpackage options at {defpackage_path}; only direct option lists are supported"
        );
    }
    let Some(option_head) = atom_text(&option.children[0]) else {
        anyhow::bail!(
            "cannot merge defpackage option at {}; option head must be an atom",
            defpackage_path.child(option_index)
        );
    };

    let name = package_option_name(option_head);
    let body_atoms = option
        .children
        .iter()
        .skip(1)
        .map(|child| {
            atom_text(child).map(str::to_owned).with_context(|| {
                format!(
                    "cannot merge defpackage option at {}; option payload must contain atoms only",
                    defpackage_path.child(option_index)
                )
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let Some(key) = super::merge::merge_key(&name, &body_atoms) else {
        return Ok(None);
    };

    Ok(Some(OptionSlot {
        path: defpackage_path.child(option_index).to_string(),
        span: option.span,
        head_text: option_head.to_owned(),
        name,
        key,
        body_atoms,
    }))
}
