use anyhow::Result;

use super::analyze::{analyze_defpackage_form, analyze_in_package_form};
use super::types::{InPackageReport, PackageDefinitionReport, PackageReport};
use crate::domain::{
    dialect::Dialect,
    sexpr::{ExpressionView, Path, SyntaxTree},
};

pub fn build_package_report(tree: &SyntaxTree, dialect: Dialect) -> Result<PackageReport> {
    let mut defpackages = Vec::new();
    let mut in_packages = Vec::new();

    for index in 0..tree.root_children().len() {
        let path = Path::root_child(index);
        let view = tree.select_path(&path)?.view();
        collect_package_forms_from_view(&view, path, dialect, &mut defpackages, &mut in_packages)?;
    }

    Ok(PackageReport::new(defpackages, in_packages))
}

fn collect_package_forms_from_view(
    view: &ExpressionView,
    path: Path,
    dialect: Dialect,
    defpackages: &mut Vec<PackageDefinitionReport>,
    in_packages: &mut Vec<InPackageReport>,
) -> Result<()> {
    if let Some(defpackage) = analyze_defpackage_form(view, dialect, &path)? {
        defpackages.push(defpackage);
    }
    if let Some(in_package) = analyze_in_package_form(view, dialect, &path)? {
        in_packages.push(in_package);
    }

    for (index, child) in view.children.iter().enumerate() {
        let child_path = path.child(index);
        collect_package_forms_from_view(child, child_path, dialect, defpackages, in_packages)?;
    }

    Ok(())
}
