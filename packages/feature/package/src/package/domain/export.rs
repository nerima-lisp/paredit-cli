use anyhow::Result;

use crate::domain::{
    common_lisp::CommonLispPackageDeclarationForm,
    dialect::Dialect,
    sexpr::{
        ByteOffset, ByteSpan, Delimiter, ExpressionKind, ExpressionView, Path, SymbolName,
        SyntaxTree,
    },
};

use super::syntax::{
    atom_text, is_package_head, normalize_package_atom, package_atoms_match, package_option_atoms,
    package_option_name,
};

#[derive(Debug)]
pub(super) struct DefpackageExportEdit {
    pub(super) defpackage_path: String,
    pub(super) defpackage_span: ByteSpan,
    pub(super) package_name: String,
    pub(super) export_span: Option<ByteSpan>,
    pub(super) insertion_span: ByteSpan,
    pub(super) already_exported: bool,
    pub(super) replacement: String,
}

pub(super) fn find_defpackage_export_edit(
    tree: &SyntaxTree,
    dialect: Dialect,
    package: Option<&SymbolName>,
    symbol: &SymbolName,
) -> Result<DefpackageExportEdit> {
    let mut matches = Vec::new();

    for index in 0..tree.root_children().len() {
        let path = Path::root_child(index);
        let view = tree.select_path(&path)?.view();
        collect_defpackage_export_edits(&view, path, dialect, package, symbol, &mut matches);
    }

    if matches.is_empty() {
        let target = package.map_or("any package".to_owned(), |package| package.to_string());
        anyhow::bail!("no matching defpackage form found for {target}");
    }
    if matches.len() > 1 {
        anyhow::bail!(
            "multiple matching defpackage forms found; pass --package to choose one unambiguously"
        );
    }

    Ok(matches.remove(0))
}

fn collect_defpackage_export_edits(
    view: &ExpressionView,
    path: Path,
    dialect: Dialect,
    package: Option<&SymbolName>,
    symbol: &SymbolName,
    matches: &mut Vec<DefpackageExportEdit>,
) {
    if let Some(edit) = analyze_defpackage_export_edit(view, &path, dialect, package, symbol) {
        matches.push(edit);
    }

    for (index, child) in view.children.iter().enumerate() {
        collect_defpackage_export_edits(
            child,
            path.child(index),
            dialect,
            package,
            symbol,
            matches,
        );
    }
}

fn analyze_defpackage_export_edit(
    view: &ExpressionView,
    path: &Path,
    dialect: Dialect,
    package: Option<&SymbolName>,
    symbol: &SymbolName,
) -> Option<DefpackageExportEdit> {
    if view.kind != ExpressionKind::List || view.delimiter != Some(Delimiter::Paren) {
        return None;
    }
    if view.children.len() < 2 {
        return None;
    }
    let head = atom_text(&view.children[0])?;
    if !is_package_head(dialect, head, CommonLispPackageDeclarationForm::Defpackage) {
        return None;
    }

    let package_name = atom_text(&view.children[1])?.to_owned();
    if package.is_some_and(|package| !package_atoms_match(&package_name, package.as_str())) {
        return None;
    }

    let mut export_option = None;
    for option in view.children.iter().skip(2) {
        if option.kind != ExpressionKind::List || option.children.is_empty() {
            continue;
        }
        let Some(option_head) = atom_text(&option.children[0]) else {
            continue;
        };
        if package_option_name(option_head) == "export" {
            export_option = Some(option);
            break;
        }
    }

    let (export_span, insertion_offset, already_exported, replacement) =
        if let Some(option) = export_option {
            let already_exported = package_option_atoms(option)
                .skip(1)
                .any(|export| package_atoms_match(&export, symbol.as_str()));
            let insertion_offset = option.span.end().get().saturating_sub(1);
            let replacement = if already_exported {
                String::new()
            } else {
                format!(" #:{}", normalize_package_atom(symbol.as_str()))
            };
            (
                Some(option.span),
                insertion_offset,
                already_exported,
                replacement,
            )
        } else {
            let insertion_offset = view.span.end().get().saturating_sub(1);
            let replacement = format!(
                "\n  (:export #:{})",
                normalize_package_atom(symbol.as_str())
            );
            (None, insertion_offset, false, replacement)
        };
    let insertion_span = ByteSpan::new(
        ByteOffset::new(insertion_offset),
        ByteOffset::new(insertion_offset),
    );

    Some(DefpackageExportEdit {
        defpackage_path: path.to_string(),
        defpackage_span: view.span,
        package_name,
        export_span,
        insertion_span,
        already_exported,
        replacement,
    })
}
