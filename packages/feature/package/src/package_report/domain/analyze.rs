use crate::error::PackageRefactorResult;

use super::syntax::{atom_text, is_package_head, package_option_atoms, package_option_name};
use super::types::{InPackageReport, PackageDefinitionReport, PackageImportReport};
use paredit_core_syntax::common_lisp::CommonLispPackageDeclarationForm;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{Delimiter, ExpressionKind, ExpressionView, Path};

pub fn analyze_defpackage_form(
    view: &ExpressionView,
    dialect: Dialect,
    path: &Path,
) -> PackageRefactorResult<Option<PackageDefinitionReport>> {
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

pub fn analyze_in_package_form(
    view: &ExpressionView,
    dialect: Dialect,
    path: &Path,
) -> PackageRefactorResult<Option<InPackageReport>> {
    if view.kind != ExpressionKind::List || view.delimiter != Some(Delimiter::Paren) {
        return Ok(None);
    }
    if view.children.len() < 2 {
        return Ok(None);
    }
    let Some(head) = atom_text(&view.children[0]) else {
        return Ok(None);
    };
    if !is_package_head(dialect, head, CommonLispPackageDeclarationForm::InPackage)
        && !is_clojure_namespace_head(dialect, head)
    {
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

/// Returns whether a head declares the namespace a Clojure file's forms belong
/// to.
///
/// `ns` and `in-ns` fill the role Common Lisp splits between `defpackage` and
/// `in-package`, but only the `in-package` half is reported here. The
/// `defpackage` half — the `:require`/`:use`/`:import` clauses — is already
/// reported by the dependency layer as `ns-require`/`ns-use`/`ns-import`, so
/// reporting it again as a package *definition* would both double-count those
/// edges and expose an `ns` form to the `defpackage` export and nickname
/// refactors, which cannot rewrite it correctly.
fn is_clojure_namespace_head(dialect: Dialect, head: &str) -> bool {
    dialect == Dialect::Clojure
        && paredit_core_syntax::clojure::ClojureOperator::from_head(head)
            .is_some_and(paredit_core_syntax::clojure::ClojureOperator::is_namespace_declaration)
}
