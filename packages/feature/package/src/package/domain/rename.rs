use anyhow::Result;

use crate::domain::{
    dialect::Dialect,
    sexpr::{Path, SymbolName, SyntaxTree},
};

use super::PackageRenameOccurrence;

mod occurrences;
mod paths;
mod replacement;

use occurrences::collect_package_rename_occurrences;

pub(super) fn package_rename_occurrences(
    tree: &SyntaxTree,
    dialect: Dialect,
    from: &SymbolName,
    to: &SymbolName,
) -> Result<Vec<PackageRenameOccurrence>> {
    let mut occurrences = Vec::new();

    for index in 0..tree.root_children().len() {
        let path = Path::root_child(index);
        let view = tree.select_path(&path)?.view();
        collect_package_rename_occurrences(&view, path, dialect, from, to, &mut occurrences);
    }

    occurrences.sort_by_key(|occurrence| occurrence.span.start());
    Ok(occurrences)
}
