use anyhow::Result;

use super::syntax::{atom_text, is_package_head, package_option_atoms, package_option_name};
use super::types::{InPackageReport, PackageDefinitionReport, PackageImportReport};
use crate::domain::{
    common_lisp::CommonLispPackageDeclarationForm,
    dialect::Dialect,
    sexpr::{Delimiter, ExpressionKind, ExpressionView, Path},
};

pub(super) fn analyze_defpackage_form(
    view: &ExpressionView,
    dialect: Dialect,
    path: &Path,
) -> Result<Option<PackageDefinitionReport>> {
    if view.kind != ExpressionKind::List || view.delimiter != Some(Delimiter::Paren) {
        return Ok(None);
    }
    if view.children.len() < 2 {
        return Ok(None);
    }
    let Some(head) = atom_text(&view.children[0]) else {
        return Ok(None);
    };
    if !is_package_head(dialect, head, CommonLispPackageDeclarationForm::Defpackage) {
        return Ok(None);
    }

    let Some(name) = atom_text(&view.children[1]) else {
        // A non-atom package designator (for example a quasiquoted
        // `(defpackage ,name ...)` template) has no statically resolvable name,
        // so it is not a real declaration. Skip it instead of failing the report.
        return Ok(None);
    };
    let name = name.to_owned();
    let mut nicknames = Vec::new();
    let mut uses = Vec::new();
    let mut exports = Vec::new();
    let mut imports = Vec::new();
    let mut option_count = 0;

    for option in view.children.iter().skip(2) {
        if option.kind != ExpressionKind::List || option.children.is_empty() {
            continue;
        }
        let Some(option_head) = atom_text(&option.children[0]) else {
            continue;
        };
        option_count += 1;
        match package_option_name(option_head).as_str() {
            "nicknames" => nicknames.extend(package_option_atoms(option).skip(1)),
            "use" => uses.extend(package_option_atoms(option).skip(1)),
            "export" => exports.extend(package_option_atoms(option).skip(1)),
            "import-from" => {
                let mut atoms = package_option_atoms(option).skip(1);
                if let Some(package) = atoms.next() {
                    imports.push(PackageImportReport::new(package, atoms.collect()));
                }
            }
            _ => {}
        }
    }

    Ok(Some(PackageDefinitionReport::new(
        path.to_string(),
        view.span,
        name,
        nicknames,
        uses,
        exports,
        imports,
        option_count,
    )))
}

pub(super) fn analyze_in_package_form(
    view: &ExpressionView,
    dialect: Dialect,
    path: &Path,
) -> Result<Option<InPackageReport>> {
    if view.kind != ExpressionKind::List || view.delimiter != Some(Delimiter::Paren) {
        return Ok(None);
    }
    if view.children.len() < 2 {
        return Ok(None);
    }
    let Some(head) = atom_text(&view.children[0]) else {
        return Ok(None);
    };
    if !is_package_head(dialect, head, CommonLispPackageDeclarationForm::InPackage) {
        return Ok(None);
    }

    let Some(name) = atom_text(&view.children[1]) else {
        // A non-atom package designator (for example a quasiquoted
        // `(in-package ,pkg)` code template emitted into a stream) cannot be
        // resolved to a static package name and is not a dependency edge. Skip
        // it instead of failing the whole report.
        return Ok(None);
    };
    let name = name.to_owned();

    Ok(Some(InPackageReport::new(
        path.to_string(),
        view.span,
        name,
    )))
}
