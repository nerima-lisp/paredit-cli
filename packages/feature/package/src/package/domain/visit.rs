use anyhow::{Context, Result};

use crate::domain::{
    common_lisp::CommonLispPackageDeclarationForm,
    dialect::Dialect,
    sexpr::{Delimiter, ExpressionKind, ExpressionView, Path, SymbolName, SyntaxTree},
};

use super::syntax::{atom_text, is_package_head, package_atoms_match};

pub(super) fn visit_defpackage_forms(
    tree: &SyntaxTree,
    dialect: Dialect,
    package: Option<&SymbolName>,
    mut visitor: impl FnMut(&ExpressionView, &Path, &str) -> Result<()>,
) -> Result<()> {
    let mut matched_defpackages = 0usize;

    for index in 0..tree.root_children().len() {
        let path = Path::root_child(index);
        let view = tree.select_path(&path)?.view();
        visit_form(
            &view,
            &path,
            dialect,
            package,
            &mut matched_defpackages,
            &mut visitor,
        )
        .with_context(|| format!("failed to inspect package form at {path}"))?;
    }

    if matched_defpackages == 0 {
        if let Some(target) = package {
            anyhow::bail!("no matching defpackage form found for {target}");
        }
    }

    Ok(())
}

fn visit_form(
    view: &ExpressionView,
    path: &Path,
    dialect: Dialect,
    package: Option<&SymbolName>,
    matched_defpackages: &mut usize,
    visitor: &mut impl FnMut(&ExpressionView, &Path, &str) -> Result<()>,
) -> Result<()> {
    if view.kind == ExpressionKind::List
        && view.delimiter == Some(Delimiter::Paren)
        && view.children.len() >= 2
        && atom_text(&view.children[0]).is_some_and(|head| {
            is_package_head(dialect, head, CommonLispPackageDeclarationForm::Defpackage)
        })
    {
        if let Some(package_name) = atom_text(&view.children[1]) {
            if package.is_none_or(|target| package_atoms_match(package_name, target.as_str())) {
                *matched_defpackages += 1;
                visitor(view, path, package_name)?;
            }
        }
    }

    for (index, child) in view.children.iter().enumerate() {
        visit_form(
            child,
            &path.child(index),
            dialect,
            package,
            matched_defpackages,
            visitor,
        )?;
    }

    Ok(())
}
