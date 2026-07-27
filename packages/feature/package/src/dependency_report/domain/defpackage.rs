use crate::domain::package_report::PackageDefinitionReport;

use super::types::{DependencyKind, DependencyReportItem};

pub(super) fn defpackage_dependency_items(
    defpackages: &[PackageDefinitionReport],
) -> Vec<DependencyReportItem> {
    let mut dependencies = Vec::new();

    for defpackage in defpackages {
        for target in &defpackage.uses {
            dependencies.push(DependencyReportItem::new(
                DependencyKind::DefpackageUse,
                target.clone(),
                defpackage.path.clone(),
                defpackage.span,
                Some(defpackage.name.clone()),
            ));
        }
        for import in &defpackage.imports {
            dependencies.push(DependencyReportItem::new(
                DependencyKind::DefpackageImportFrom,
                import.package.clone(),
                defpackage.path.clone(),
                defpackage.span,
                Some(defpackage.name.clone()),
            ));
        }
    }

    dependencies
}
