//! Dependency inventory analysis for Lisp source forms and package declarations.

use anyhow::Result;

use crate::package_report::domain::build_package_report;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;

mod collect;
mod defpackage;
mod syntax;
#[cfg(test)]
mod tests;
mod types;

pub use types::{DependencyKind, DependencyReport, DependencyReportItem};

use collect::{collect_dependency_items, collect_system_dependency_edges};
use defpackage::defpackage_dependency_items;

pub fn build_dependency_report(tree: &SyntaxTree, dialect: Dialect) -> Result<DependencyReport> {
    let package_report = build_package_report(tree, dialect)?;
    let mut dependencies = collect_dependency_items(tree, dialect)?;
    dependencies.extend(defpackage_dependency_items(&package_report.defpackages));
    dependencies.sort_by(DependencyReportItem::cmp_position);

    Ok(DependencyReport::new(dependencies))
}

/// Collects `(declaring_system, depended_on_system)` edges from every
/// top-level ASDF `defsystem` form in `tree`, for cross-file system
/// dependency-cycle analysis.
pub fn build_system_dependency_edges(
    tree: &SyntaxTree,
    dialect: Dialect,
) -> Result<Vec<(String, String)>> {
    collect_system_dependency_edges(tree, dialect)
}
