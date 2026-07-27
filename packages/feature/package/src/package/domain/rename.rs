use crate::error::PackageRefactorResult;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{Path, SymbolName, SyntaxTree};

use super::PackageRenameOccurrence;

mod occurrences;
mod paths;
mod replacement;

use occurrences::collect_package_rename_occurrences;

pub fn package_rename_occurrences(
    tree: &SyntaxTree,
    dialect: Dialect,
    from: &SymbolName,
    to: &SymbolName,
) -> PackageRefactorResult<Vec<PackageRenameOccurrence>> {
    let mut occurrences = Vec::new();

    for index in 0..tree.root_children().len() {
        let path = Path::root_child(index);
        let view = tree.select_path(&path)?.view();
        collect_package_rename_occurrences(&view, path, dialect, from, to, &mut occurrences);
    }

    occurrences.sort_by_key(|occurrence| occurrence.span.start());
    Ok(occurrences)
}
