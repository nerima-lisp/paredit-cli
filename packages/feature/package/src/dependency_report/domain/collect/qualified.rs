use paredit_core_syntax::common_lisp::common_lisp_symbol_name_eq;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ExpressionView, Path};

use crate::dependency_report::domain::syntax::package_qualified_dependency_target;
use crate::dependency_report::domain::types::{DependencyKind, DependencyReportItem};

pub fn collect_qualified_symbol_dependency(
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
