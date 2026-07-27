use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{Path, SymbolName, SyntaxTree};

use super::RenameFunctionOccurrence;

mod binding;
mod call;
mod reader;
mod scope;
mod traversal;

use binding::collect_macrolet_binding_renames_from_view;
use call::collect_macrolet_call_head_renames_from_view;
use scope::{LocalCallableRenameKind, MacroletRenameScope};

type RenameCollector = fn(
    &crate::domain::sexpr::ExpressionView,
    Path,
    Dialect,
    &SymbolName,
    &SymbolName,
    LocalCallableRenameKind,
    MacroletRenameScope,
    MacroletRenameScope,
    usize,
    &mut Vec<RenameFunctionOccurrence>,
);

fn collect_renames(
    tree: &SyntaxTree,
    dialect: Dialect,
    from: &SymbolName,
    to: &SymbolName,
    kind: LocalCallableRenameKind,
    collect_from_view: RenameCollector,
) -> Result<Vec<RenameFunctionOccurrence>> {
    let mut renames = Vec::new();

    for (index, _) in tree.root_children().iter().enumerate() {
        let path = Path::root_child(index);
        let view = tree.select_path(&path)?.view();
        collect_from_view(
            &view,
            path,
            dialect,
            from,
            to,
            kind,
            MacroletRenameScope::default(),
            MacroletRenameScope::default(),
            0,
            &mut renames,
        );
    }

    renames.sort_by_key(|rename| rename.span.start());
    Ok(renames)
}

pub fn collect_macrolet_binding_renames(
    tree: &SyntaxTree,
    dialect: Dialect,
    from: &SymbolName,
    to: &SymbolName,
) -> Result<Vec<RenameFunctionOccurrence>> {
    collect_renames(
        tree,
        dialect,
        from,
        to,
        LocalCallableRenameKind::Macro,
        collect_macrolet_binding_renames_from_view,
    )
}

pub fn collect_macrolet_call_head_renames(
    tree: &SyntaxTree,
    dialect: Dialect,
    from: &SymbolName,
    to: &SymbolName,
) -> Result<Vec<RenameFunctionOccurrence>> {
    collect_renames(
        tree,
        dialect,
        from,
        to,
        LocalCallableRenameKind::Macro,
        collect_macrolet_call_head_renames_from_view,
    )
}

pub fn collect_local_function_binding_renames(
    tree: &SyntaxTree,
    dialect: Dialect,
    from: &SymbolName,
    to: &SymbolName,
) -> Result<Vec<RenameFunctionOccurrence>> {
    collect_renames(
        tree,
        dialect,
        from,
        to,
        LocalCallableRenameKind::Function,
        collect_macrolet_binding_renames_from_view,
    )
}

pub fn collect_local_function_call_head_renames(
    tree: &SyntaxTree,
    dialect: Dialect,
    from: &SymbolName,
    to: &SymbolName,
) -> Result<Vec<RenameFunctionOccurrence>> {
    collect_renames(
        tree,
        dialect,
        from,
        to,
        LocalCallableRenameKind::Function,
        collect_macrolet_call_head_renames_from_view,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_setf_local_function_definition_rename() {
        let tree = SyntaxTree::parse(
            "(flet (((setf foo) (value object) value)) ((setf foo) 1 thing) foo)\n",
        )
        .expect("test input should parse");
        let from = SymbolName::new("foo").expect("symbol");
        let to = SymbolName::new("bar").expect("symbol");

        let renames =
            collect_local_function_binding_renames(&tree, Dialect::CommonLisp, &from, &to)
                .expect("collector should succeed");

        assert_eq!(renames.len(), 1);
        assert_eq!(renames[0].path, "0.1.0.0.1");
    }
}
