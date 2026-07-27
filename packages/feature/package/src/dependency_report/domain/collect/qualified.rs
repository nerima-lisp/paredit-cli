use crate::domain::common_lisp::common_lisp_symbol_name_eq;
use crate::domain::sexpr::reader::atom_symbol_text;
use crate::domain::sexpr::{ExpressionView, Path};

use crate::domain::dependency_report::syntax::package_qualified_dependency_target;
use crate::domain::dependency_report::types::{DependencyKind, DependencyReportItem};

pub(super) fn collect_qualified_symbol_dependency(
    view: &ExpressionView,
    path: &Path,
    local_bindings: &[String],
    dependencies: &mut Vec<DependencyReportItem>,
) {
    let Some(atom) = atom_symbol_text(view) else {
        return;
    };
    if local_bindings
        .iter()
        .any(|binding| common_lisp_symbol_name_eq(binding, atom))
    {
        return;
    }
    let Some(target) = package_qualified_dependency_target(atom) else {
        return;
    };

    dependencies.push(DependencyReportItem::new(
        DependencyKind::QualifiedSymbol,
        target,
        path.to_string(),
        view.span,
        Some(atom.to_owned()),
    ));
}
