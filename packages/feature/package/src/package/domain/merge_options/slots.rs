use crate::error::{DefpackageShapeError, PackageRefactorResult};

use paredit_core_syntax::sexpr::{ExpressionKind, ExpressionView, Path};

use super::OptionSlot;
use crate::package::domain::syntax::{atom_text, package_option_name};

pub fn collect_option_slots(
    view: &ExpressionView,
    defpackage_path: &Path,
) -> PackageRefactorResult<Vec<OptionSlot>> {
    view.children
        .iter()
        .enumerate()
        .skip(2)
        .map(|(option_index, option)| analyze_option_slot(option, defpackage_path, option_index))
        .collect::<PackageRefactorResult<Vec<_>>>()
        .map(|slots| slots.into_iter().flatten().collect())
}

fn analyze_option_slot(
    option: &ExpressionView,
    defpackage_path: &Path,
    option_index: usize,
) -> PackageRefactorResult<Option<OptionSlot>> {
    if option.kind != ExpressionKind::List || option.children.is_empty() {
        return Err(DefpackageShapeError::MergeOptionsNotDirectLists {
            path: defpackage_path.to_string(),
        }
        .into());
    }
    let Some(option_head) = atom_text(&option.children[0]) else {
        return Err(DefpackageShapeError::MergeOptionHeadNotAtom {
            path: defpackage_path.child(option_index).to_string(),
        }
        .into());
    };

    let name = package_option_name(option_head);
    let body_atoms = option
        .children
        .iter()
        .skip(1)
        .map(|child| {
            atom_text(child).map(str::to_owned).ok_or_else(|| {
                DefpackageShapeError::MergeOptionPayloadNotAtoms {
                    path: defpackage_path.child(option_index).to_string(),
                }
                .into()
            })
        })
        .collect::<PackageRefactorResult<Vec<_>>>()?;

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
